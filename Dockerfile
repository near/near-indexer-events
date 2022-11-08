FROM rust:1.62 AS builder
WORKDIR /tmp/
COPY Cargo.toml Cargo.lock ./
COPY src/lib.rs src/lib.rs

# this build step will cache your dependencies
RUN cargo build --release && rm -r src

# copy your source tree
COPY ./src ./src

# build for release
RUN cargo build --release

FROM ubuntu:20.04
RUN apt update && apt install -yy openssl ca-certificates
COPY --from=builder /tmp/target/release/indexer-events .
ENTRYPOINT ["./indexer-events"]
