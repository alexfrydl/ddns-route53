FROM rust AS builder
WORKDIR /app
COPY . .
RUN cargo install --path . --root /

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y openssl ca-certificates
COPY --from=builder /bin/ddns-route53 /bin/
ENTRYPOINT ["ddns-route53"]