FROM rust

WORKDIR /usr/src/myapp
COPY src ./src
COPY Cargo.toml .
COPY entrypoint.sh /usr/local/bin/
RUN chmod +x /usr/local/bin/entrypoint.sh

CMD ["entrypoint.sh"]
# CMD sleep 30000

###############################################################
###############################################################
###############################################################

# To deploy, run `cross build --release --target x86_64-unknown-linux-musl` locally
# bumpversion and merge to dev

# # Our Second stage, that will be the final image
# FROM scratch

# # copy the build artifact from the build stage
# COPY ./target/x86_64-unknown-linux-musl/release/btc-indexer .

# # set the startup command to run your binary
# CMD ["./btc-indexer"]

###############################################################
###############################################################
###############################################################

# ################
# # Builder
# ################

# # Define the build stage
# FROM rust:1.70 as builder
# WORKDIR /usr/src

# # Install cross, a cargo wrapper for cross-compilation
# RUN cargo install cross

# # Install musl-tools for static compilation
# # RUN apt-get update && apt-get install -y libssl-dev musl-tools pkg-config
# # RUN rustup target add x86_64-unknown-linux-musl

# # Create a new empty shell project
# RUN USER=root cargo new omnisat-indexer-rs
# WORKDIR /usr/src/omnisat-indexer-rs

# # Copy over your manifests
# COPY ./Cargo.lock ./Cargo.lock
# COPY ./Cargo.toml ./Cargo.toml

# # Copy your source tree
# COPY ./src ./src

# # RUN cargo build --release
# # Build for release. Use the musl target for static compilation
# # RUN cargo install --target x86_64-unknown-linux-musl --path .
# RUN cross build --release --target x86_64-unknown-linux-musl

# ################
# # Runner
# ################

# # Our Second stage, that will be the final image
# FROM scratch

# # copy the build artifact from the build stage
# COPY --from=builder /usr/src/omnisat-indexer-rs/target/x86_64-unknown-linux-musl/release/btc-indexer .

# # set the startup command to run your binary
# CMD ["./btc-indexer"]
