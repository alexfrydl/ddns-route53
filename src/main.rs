use std::{collections::HashMap, net::Ipv4Addr, process::exit, time::Duration};

use anyhow::{bail, Context, Result};
use aws_sdk_route53::{
  self as route53,
  types::{Change, ChangeAction::Upsert, ChangeBatch, ResourceRecord, ResourceRecordSet, RrType},
};
use clap::Parser;

/// Basic log macro.
macro_rules! log {
  ($($args:tt)*) => {
    {
      print!("[{}] ", chrono::Utc::now().format("%F %T"));
      println!($($args)*);
    }
  };
}

/// Basic error log macro.
macro_rules! log_err {
  ($($args:tt)*) => {
    {
      eprint!("[{}] ERROR — ", chrono::Utc::now().format("%F %T"));
      eprintln!($($args)*);
    }
  };
}

#[derive(Parser)]
#[command(version, about)]
struct Args {
  /// Domain names to update
  #[arg(required = true)]
  domains: Vec<String>,
  /// Run as a daemon that updates DNS records whenever the public IP changes.
  #[arg(short, long)]
  daemon: bool,
}

struct App {
  domains: HashMap<String, Domain>,
  is_daemon: bool,
  public_ip: String,
  route53: route53::Client,
}

struct Domain {
  needs_update: bool,
  zone_id: String,
}

#[tokio::main]
async fn main() -> Result<()> {
  App::new(Args::parse()).await?.run().await;
  Ok(())
}

impl App {
  async fn new(args: Args) -> Result<Self> {
    let mut domains = HashMap::new();

    for name in args.domains {
      if name.len() < 3 || !name.contains('.') {
        bail!("invalid domain name {name:?}");
      }

      domains.insert(name, Domain::new());
    }

    let aws_config = aws_config::load_from_env().await;
    let route53 = route53::Client::new(&aws_config);

    Ok(Self {
      domains,
      is_daemon: args.daemon,
      public_ip: String::new(),
      route53,
    })
  }

  async fn run(&mut self) {
    loop {
      self.refresh_public_ip().await;
      self.update_dns().await;

      if !self.is_daemon {
        return;
      }

      tokio::time::sleep(Duration::from_secs(5)).await;
    }
  }

  async fn refresh_public_ip(&mut self) {
    match get()
      .await
      .with_context(|| "Failed to determine public IP.")
    {
      Ok(ip) => {
        if ip != self.public_ip {
          if self.public_ip.is_empty() {
            log!("Public IP is {ip}.");
          } else {
            log!("Public IP has changed to {ip}.");
          }

          for domain in self.domains.values_mut() {
            domain.needs_update = true;
          }
        }

        self.public_ip = ip;
      }

      Err(err) => {
        log_err!("{err:?}");

        if !self.is_daemon {
          exit(1);
        }
      }
    }

    async fn get() -> Result<String> {
      let mut ip_string = reqwest::get("https://api.ipify.org").await?.text().await?;

      ip_string.truncate(16);
      ip_string = ip_string.trim().to_string();
      ip_string.parse::<Ipv4Addr>()?;

      Ok(ip_string)
    }
  }

  async fn update_dns(&mut self) {
    if !self.domains.values().any(|d| d.needs_update) {
      return;
    }

    // get list of hosted zones

    let zones = match self
      .route53
      .list_hosted_zones()
      .send()
      .await
      .with_context(|| "Failed to list Route 53 hosted zones.")
    {
      Ok(list) => list.hosted_zones,

      Err(err) => {
        log_err!("{err:?}");

        if !self.is_daemon {
          exit(1);
        }

        return;
      }
    };

    // match domain names to hosted zones

    for (name, state) in &mut self.domains {
      if !state.needs_update {
        continue;
      }

      let Some(zone) = zones
        .iter()
        // find hosted zones that could contain this domain name
        .filter(|z| match name.strip_suffix(z.name.trim_end_matches('.')) {
          Some(rest) => rest.is_empty() || rest.ends_with('.'),
          None => false,
        })
        // pick the hosted zone with the deepest subdomain match
        .max_by_key(|z| z.name.len())
      else {
        log_err!("Cannot find a hosted zone for `{name}`.");

        if !self.is_daemon {
          exit(1);
        }

        continue;
      };

      state.zone_id.replace_range(.., &zone.id);
    }

    // update DNS records

    for (name, state) in &mut self.domains {
      if !state.needs_update || state.zone_id.is_empty() {
        continue;
      }

      match upsert(&self.route53, &state.zone_id, name, &self.public_ip)
        .await
        .with_context(|| format!("Failed to update `{name}`."))
      {
        Ok(()) => {
          state.needs_update = false;
          log!("Updated `{name}` to {}.", &self.public_ip);
        }

        Err(err) => {
          log_err!("{err:?}");

          if !self.is_daemon {
            exit(1);
          }
        }
      }
    }

    async fn upsert(
      route53: &route53::Client,
      zone_id: impl Into<String>,
      name: impl Into<String>,
      ip: impl Into<String>,
    ) -> Result<()> {
      route53
        .change_resource_record_sets()
        .hosted_zone_id(zone_id)
        .change_batch(
          ChangeBatch::builder()
            .changes(
              Change::builder()
                .action(Upsert)
                .resource_record_set(
                  ResourceRecordSet::builder()
                    .r#type(RrType::A)
                    .name(name)
                    .resource_records(ResourceRecord::builder().value(ip).build()?)
                    .ttl(300)
                    .build()?,
                )
                .build()?,
            )
            .build()?,
        )
        .send()
        .await?;

      Ok(())
    }
  }
}

impl Domain {
  const fn new() -> Self {
    Self {
      needs_update: true,
      zone_id: String::new(),
    }
  }
}
