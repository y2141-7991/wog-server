# syntax=docker/dockerfile:1.7

# ---- Build -----------------------------------------------------------------
FROM rust:1-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
# `sqlx::migrate!` embeds the migration files into the binary at compile time, so
# they are needed here but not in the runtime image.
COPY migrations/ migrations/

# BuildKit cache mounts keep the crate downloads and the incremental build between
# builds, so a code change recompiles only what changed. The binary is copied out of
# the cache mount because the mount is not part of the image.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked -p wog-api --bin server \
    && cp target/release/server /usr/local/bin/server

# ---- Runtime ---------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# ca-certificates: TLS to Google, SMTP and SePay.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd -r -g 10001 app && useradd -r -u 10001 -g app -s /usr/sbin/nologin app

WORKDIR /app
COPY --from=builder /usr/local/bin/server /usr/local/bin/server

# Inside a container the server has to listen on every interface, otherwise the
# published port (and the client container) cannot reach it. Override with
# SERVER_HOST / SERVER_PORT if needed; an `.env` value of 127.0.0.1 is overridden in
# docker-compose.yml for the same reason.
ENV SERVER_HOST=0.0.0.0 \
    SERVER_PORT=3000 \
    RUST_LOG=info

# Numeric, so Kubernetes (runAsNonRoot) can verify the user is not root.
USER 10001:10001
EXPOSE 3000

# A plain TCP connect (bash is in the image, curl is not); /helloworld would also
# work but writes a log line on every probe.
HEALTHCHECK --interval=30s --timeout=3s --start-period=20s --retries=3 \
    CMD ["bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/${SERVER_PORT:-3000}"]

CMD ["server"]
