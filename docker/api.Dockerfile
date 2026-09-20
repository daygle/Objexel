FROM node:25-slim AS web
WORKDIR /web
COPY web/package.json ./
RUN npm install
COPY web/ .
RUN npm run build

FROM rust:1-trixie AS builder
WORKDIR /src
COPY . .
ARG FEATURES=""
RUN cargo build --release -p objexel-api ${FEATURES:+--features ${FEATURES}}

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates ffmpeg wget tini && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/objexel-api /usr/local/bin/objexel-api
COPY --from=web /web/build /usr/local/share/objexel/web
EXPOSE 8080
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/objexel-api"]
