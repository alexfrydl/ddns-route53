# ddns-route53

A tiny daemon that monitors the host's public IP for changes and updates one or
more records in Amazon Route 53.

This can be used to implement [dynamic DNS][1] for your own domains.

## Example

```
> ddns-route53 example1.com test.example2.com
[2024-09-20 19:24:12] Public IP is 123.123.123.123.
[2024-09-20 19:24:12] Updated `example1.com` to 123.123.123.123.
[2024-09-20 19:24:12] Updated `test.example2.com` to 123.123.123.123.
```

## Details

AWS credentials are loaded from the environment. Every five minutes, the daemon uses [ipify.org][2] to determine the host's current public IP. Whenever the IP changes, the daemon updates the A records of the domain names given as command line arguments.

[1]: https://en.wikipedia.org/wiki/Dynamic_DNS
[2]: https://ipify.org