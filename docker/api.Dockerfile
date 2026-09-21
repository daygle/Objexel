FROM node:25-slim AS web
WORKDIR /web
COPY web/package.json ./
RUN npm install
COPY web/ .
RUN npm run build

FROM rust:1-trixie AS builder
RUN apt-get update && apt-get install -y --no-install-recommends cmake g++ git python3 libeigen3-dev && rm -rf /var/lib/apt/lists/*
RUN git clone --branch v1.16.3 --depth 1 https://github.com/microsoft/onnxruntime.git /tmp/onnxruntime
WORKDIR /tmp/onnxruntime
RUN ./build.sh --build_shared_lib --skip_tests --config Release --allow_running_as_root --use_preinstalled_eigen --cmake_extra_defines eigen_SOURCE_PATH=/usr/include/eigen3
RUN cp build/Linux/Release/libonnxruntime.so* /usr/local/lib/
WORKDIR /src
COPY . .
ENV ORT_LIB_LOCATION=/usr/local
ARG FEATURES=""
RUN cargo build --release -p objexel-api ${FEATURES:+--features ${FEATURES}}

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates ffmpeg wget tini && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/lib/libonnxruntime.so* /usr/local/lib/
COPY --from=builder /src/target/release/objexel-api /usr/local/bin/objexel-api
COPY --from=web /web/build /usr/local/share/objexel/web
EXPOSE 8080
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/objexel-api"]
