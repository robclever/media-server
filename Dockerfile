FROM rust:1.96-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY web ./web
RUN cargo build --locked --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl ffmpeg && rm -rf /var/lib/apt/lists/* && groupadd -g 10001 cinema && useradd -u 10001 -g cinema cinema
COPY --from=builder /build/target/release/custom-plex /usr/local/bin/custom-plex
ENV APP_BIND=0.0.0.0:8080 MEDIA_DIR=/media DATA_DIR=/data
USER 10001:10001
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s CMD curl --fail --silent http://127.0.0.1:8080/health || exit 1
ENTRYPOINT ["custom-plex"]
