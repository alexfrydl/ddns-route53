use std::{collections::HashMap, net::Ipv4Addr};

use anyhow::{bail, Context, Result};
use aws_sdk_route53::{
  self as route53,
  types::{Change, ChangeAction::Upsert, ChangeBatch, ResourceRecord, ResourceRecordSet, RrType},
};
use clap::Parser;

#[derive(Parser)]
#[command(version, about)]
struct Args {
  /// Domain names to update
  #[arg(required = true)]
  domains: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
  // parse and validate args

  let args = Args::parse();

  for name in &args.domains {
    if name.len() < 3 || !name.contains('.') {
      bail!("invalid domain name {name:?}");
    }
  }

  // initialize route53 client

  let aws_config = aws_config::load_from_env().await;
  let route53 = route53::Client::new(&aws_config);

  // run

  let result = run(args, route53).await;

  if result.is_err() {
    println!(); // easier to read errors
  }

  result
}

async fn run(args: Args, route53: route53::Client) -> Result<()> {
  // match domain names to hosted zones

  println!("Matching domain names to Route 53 hosted zones…");

  let zones = route53
    .list_hosted_zones()
    .send()
    .await
    .with_context(|| "failed to list Route 53 hosted zones")?
    .hosted_zones;

  let mut domain_zones = HashMap::with_capacity(args.domains.len());

  for name in &args.domains {
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
      bail!("Cannot find a hosted zone for domain {name:?}.")
    };

    domain_zones.insert(name.clone(), zone);

    println!("  {name}: {}", zone.name());
  }

  // determine current public IP

  println!("Checking public IP…");

  let public_ip = get_public_ip()
    .await
    .with_context(|| "failed to get public IP")?;

  println!("  public_ip: {public_ip}");

  // update DNS records

  println!("Updating DNS records…");

  for (domain, zone) in domain_zones {
    upsert_dns_record(&route53, zone.id(), &domain, &public_ip)
      .await
      .with_context(|| format!("failed to update {domain}"))?;

    println!("  - {domain}");
  }

  Ok(())
}

/// Gets current public IP using an HTTP API.
async fn get_public_ip() -> Result<String> {
  let mut ip_string = reqwest::get("https://ip.me").await?.text().await?;

  ip_string.truncate(16);
  ip_string = ip_string.trim().to_string();
  ip_string.parse::<Ipv4Addr>()?;

  Ok(ip_string)
}

/// Creates or updates an A record in Route 53.
async fn upsert_dns_record(
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
