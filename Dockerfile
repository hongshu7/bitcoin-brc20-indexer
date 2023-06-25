################
# Builder
################

# Define the build stage
FROM rust:1.70 as builder
WORKDIR /usr/src

# Install cross, a cargo wrapper for cross-compilation
RUN cargo install cross

# Install musl-tools for static compilation
RUN apt-get update && apt-get install -y libssl-dev musl-tools pkg-config
RUN rustup target add x86_64-unknown-linux-musl

# Create a new empty shell project
RUN USER=root cargo new omnisat-indexer-rs
WORKDIR /usr/src/omnisat-indexer-rs

# Copy over your manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# Copy your source tree
COPY ./src ./src

# Build for release. Use the musl target for static compilation
RUN cargo build --release
RUN cargo install --target x86_64-unknown-linux-musl --path .

################
# Runner
################

# Our Second stage, that will be the final image
FROM scratch

# copy the build artifact from the build stage
COPY --from=builder /usr/local/cargo/bin/btc-indexer .

# set the startup command to run your binary
CMD ["./btc-indexer"]
