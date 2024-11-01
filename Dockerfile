FROM rust AS builder
WORKDIR /app
COPY . .
RUN cargo install --path . --root /

FROM debian:bullseye-slim
COPY --from=builder /bin/ddns-route53 /bin/
CMD ["ddns-route53"]