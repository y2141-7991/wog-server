FROM rust:bookworm AS builder

WORKDIR /app

# Cache dependencies: copy manifests first, build a dummy to populate the cache
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN find crates -name "*.rs" -exec sh -c 'echo "fn main(){}" > "$1"' _ {} \;
RUN cargo build --release 2>/dev/null || true

# Copy real source and rebuild (only changed crates recompile)
COPY crates/ crates/
COPY migrations/ migrations/

ENV SQLX_OFFLINE=true
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

RUN groupadd -r app && useradd -r -g app app

WORKDIR /app

COPY --from=builder /app/target/release/server .
COPY --from=builder /app/migrations ./migrations

RUN chown -R app:app /app
USER app

EXPOSE 3000
CMD ["./server"]
