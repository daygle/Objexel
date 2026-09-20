FROM rust:1.81-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release -p objexel-api

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates ffmpeg && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/objexel-api /usr/local/bin/objexel-api
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/objexel-api"]
