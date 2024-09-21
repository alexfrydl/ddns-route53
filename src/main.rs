use std::{net::Ipv4Addr, time::Duration};

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
  /// Domain names to update.
  #[arg(required = true)]
  domains: Vec<String>,
}

struct App {
  current_ip: String,
  domains: Vec<Domain>,
  route53: route53::Client,
}

struct Domain {
  current_ip: String,
  name: String,
  zone_id: String,
}

#[tokio::main]
async fn main() -> Result<()> {
  let args = Args::parse();
  let mut app = App::new(args).await?;

  loop {
    if let Err(err) = app
      .refresh_public_ip()
      .await
      .with_context(|| "Failed to determine public IP.")
    {
      log_err!("{err:?}");
      continue;
    }

    app.update_dns().await;

    tokio::time::sleep(Duration::from_secs(300)).await;
  }
}

impl App {
  async fn new(args: Args) -> Result<Self> {
    let mut domains = Vec::with_capacity(args.domains.len());

    for name in args.domains {
      if name.len() < 3 || !name.contains('.') {
        bail!("Invalid domain name {name:?}.");
      }

      domains.push(Domain::new(name));
    }

    let aws_config = aws_config::load_from_env().await;
    let route53 = route53::Client::new(&aws_config);

    Ok(Self {
      domains,
      current_ip: String::new(),
      route53,
    })
  }

  async fn refresh_public_ip(&mut self) -> Result<()> {
    let mut ip = reqwest::get("https://api.ipify.org").await?.text().await?;

    ip.truncate(16);
    ip = ip.trim().to_string();
    ip.parse::<Ipv4Addr>()?;

    if ip != self.current_ip {
      if self.current_ip.is_empty() {
        log!("Public IP is {ip}.");
      } else {
        log!("Public IP has changed to {ip}.");
      }

      self.current_ip = ip;
    }

    Ok(())
  }

  async fn update_dns(&mut self) {
    if !self.domains.iter().any(|d| d.current_ip != self.current_ip) {
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
        return;
      }
    };

    // match domain names to hosted zones

    for domain in &mut self.domains {
      if domain.current_ip == self.current_ip {
        continue;
      }

      let Some(zone) = zones
        .iter()
        // find hosted zones that could contain this domain name
        .filter(
          |z| match domain.name.strip_suffix(z.name.trim_end_matches('.')) {
            Some(rest) => rest.is_empty() || rest.ends_with('.'),
            None => false,
          },
        )
        // pick the hosted zone with the deepest subdomain match
        .max_by_key(|zone| zone.name.len())
      else {
        log_err!("Cannot find a hosted zone for `{}`.", domain.name);
        continue;
      };

      domain.zone_id.replace_range(.., &zone.id);
    }

    // update DNS records

    for domain in &mut self.domains {
      if domain.zone_id.is_empty() || domain.current_ip == self.current_ip {
        continue;
      }

      match upsert(
        &self.route53,
        &domain.zone_id,
        &domain.name,
        &self.current_ip,
      )
      .await
      .with_context(|| format!("Failed to update `{}`.", domain.name))
      {
        Ok(()) => {
          domain.current_ip.replace_range(.., &self.current_ip);
          log!("Updated `{}` to {}.", domain.name, self.current_ip);
        }

        Err(err) => {
          log_err!("{err:?}");
        }
      }
    }

    async fn upsert(route53: &route53::Client, zone_id: &str, name: &str, ip: &str) -> Result<()> {
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
  const fn new(name: String) -> Self {
    Self {
      name,
      zone_id: String::new(),
      current_ip: String::new(),
    }
  }
}
