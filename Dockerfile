# Build : docker build -t url-shortener .
# Le binaire embarque les migrations SQL. MySQL et Redis restent externes.

FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/url-shortener /usr/local/bin/url-shortener
USER 65534:65534
EXPOSE 8080
ENV BIND_ADDR=0.0.0.0:8080
CMD ["url-shortener"]
