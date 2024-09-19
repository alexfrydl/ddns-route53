use std::net::Ipv4Addr;

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

  for name in args.domains {
    if name.len() < 3 || !name.contains('.') {
      bail!("invalid domain name {name:?}");
    }
  }

  // initialize route53 client

  let aws_config = aws_config::load_from_env().await;
  let route53 = route53::Client::new(&aws_config);

  // TODO match domain names to hosted zones

  let zones = route53
    .list_hosted_zones()
    .send()
    .await
    .with_context(|| "failed to list Route 53 hosted zones")?;

  for zone in zones.hosted_zones() {
    println!("{} {}", zone.id(), zone.name());
  }

  // determine current public IP

  println!("Checking public IP…");

  let public_ip = get_public_ip()
    .await
    .with_context(|| "failed to get public IP")?;

  println!("Public IP is {public_ip}.");

  // update DNS records

  upsert_dns_record(
    &route53,
    "/hostedzone/Z02543151PNSD5VEK06AZ",
    "alexfrydl.com",
    &public_ip,
  )
  .await
  .with_context(|| "failed to update DNS record")?;

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
