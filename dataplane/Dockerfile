# building this requires dynamic linking (maybe?), which won't work with clux/muslrust (maybe?)
FROM rust as ebpf-builder

RUN apt-get update
RUN apt-get install protobuf-compiler -qq

RUN apt-get update
RUN apt-get install protobuf-compiler -qq

WORKDIR /workspace

COPY . .

RUN apt-get update
RUN apt-get install -y protobuf-compiler

RUN rustup install nightly
RUN rustup component add rust-src --toolchain nightly
RUN cargo install bpf-linker

RUN cargo xtask build-ebpf --release

###
FROM clux/muslrust as app-builder

WORKDIR /workspace

COPY . .

COPY --from=ebpf-builder /workspace/target/ /workspace/target/

RUN cargo build --release

###
FROM alpine as application

LABEL org.opencontainers.image.source=https://github.com/kong/blixt
LABEL org.opencontainers.image.licenses=GPL-2.0-only

# RUN addgroup -g 1000 blixt-dataplane
# RUN adduser -D -s /bin/sh -u 1000 -G blixt-dataplane blixt-dataplane

COPY LICENSE /workspace/LICENSE
COPY --from=app-builder /workspace/target/x86_64-unknown-linux-musl/release/loader /blixt-dataplane

ENTRYPOINT ["/blixt-dataplane"]
