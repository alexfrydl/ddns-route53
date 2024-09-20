# ddns-route53

A tiny daemon that implements Dynamic DNS with Route 53, updating DNS records with your public IP whenever it changes.

## Example

```
> ddns-route53 example1.com example2.com
[2024-09-20 19:24:12] Public IP is 123.123.123.123.
[2024-09-20 19:24:12] Updated `example1.com` to 123.123.123.123.
[2024-09-20 19:24:12] Updated `example2.com` to 123.123.123.123.
```
