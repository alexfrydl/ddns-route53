use anyhow::Result;
use aws_sdk_route53::{
  self as route53,
  types::{Change, ChangeAction::Upsert, ChangeBatch, ResourceRecord, ResourceRecordSet, RrType},
};

#[tokio::main]
async fn main() -> Result<()> {
  let shared_config = aws_config::load_from_env().await;
  let route53 = route53::Client::new(&shared_config);
  let zones = route53.list_hosted_zones().send().await?;

  for zone in zones.hosted_zones() {
    println!("{} {}", zone.id(), zone.name());
  }

  let mut prev_ip = None;

  loop {
    let public_ip = get_public_ip().await?;

    if prev_ip.as_ref() == Some(&public_ip) {
      continue;
    }

    println!("{public_ip:?}");

    upsert_dns_record(
      &route53,
      "/hostedzone/Z02543151PNSD5VEK06AZ",
      "alexfrydl.com",
      &public_ip,
    )
    .await?;

    prev_ip = Some(public_ip);

    break;
  }

  Ok(())
}

async fn get_public_ip() -> Result<String> {
  let mut ip = reqwest::get("https://api.ipify.org").await?.text().await?;

  ip.truncate(ip.trim_end().len());

  Ok(ip)
}

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
