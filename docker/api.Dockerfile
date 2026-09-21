FROM node:25-slim AS web
WORKDIR /web
COPY web/package.json ./
RUN npm install
COPY web/ .
RUN npm run build

FROM rust:1-trixie AS builder
ARG BUILD_ORT_FROM_SOURCE=true
RUN if [ "$BUILD_ORT_FROM_SOURCE" = "true" ]; then \
      apt-get update && apt-get install -y --no-install-recommends cmake g++ git python3 libeigen3-dev && rm -rf /var/lib/apt/lists/*; \
    fi
RUN if [ "$BUILD_ORT_FROM_SOURCE" = "true" ]; then \
      git clone --branch v1.16.3 --depth 1 https://github.com/microsoft/onnxruntime.git /tmp/onnxruntime && \
      cd /tmp/onnxruntime && \
      ./build.sh --build_shared_lib --skip_tests --config Release --allow_running_as_root --use_preinstalled_eigen --eigen_path /usr/include/eigen3 && \
      cp build/Linux/Release/libonnxruntime.so* /usr/local/lib/; \
    fi
WORKDIR /src
COPY . .
ARG FEATURES=""
RUN if [ "$BUILD_ORT_FROM_SOURCE" = "true" ]; then \
      ORT_LIB_LOCATION=/usr/local cargo build --release -p objexel-api ${FEATURES:+--features ${FEATURES}}; \
    else \
      cargo build --release -p objexel-api ${FEATURES:+--features ${FEATURES}}; \
    fi

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates ffmpeg wget tini && rm -rf /var/lib/apt/lists/*
ARG BUILD_ORT_FROM_SOURCE=true
COPY --from=builder /usr/local/lib/libonnxruntime.so* /usr/local/lib/
COPY --from=builder /src/target/release/objexel-api /usr/local/bin/objexel-api
COPY --from=web /web/build /usr/local/share/objexel/web
EXPOSE 8080
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/objexel-api"]
