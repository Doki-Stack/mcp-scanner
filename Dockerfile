FROM rust:1.82-slim AS build
WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev git && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 1000 --user-group --no-create-home mcp-scanner
COPY --from=build /src/target/release/mcp-scanner /usr/local/bin/mcp-scanner
USER mcp-scanner
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/mcp-scanner"]
