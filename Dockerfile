################
# Builder
################

# Define the build stage
FROM rust:1.70 as builder
WORKDIR /usr/src

# Create a new empty shell project
RUN USER=root cargo new omnisat-indexer-rs
WORKDIR /usr/src/omnisat-indexer-rs

# Copy over your manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# # This build step will cache your dependencies
# RUN cargo build --release
# RUN rm src/*.rs

# Copy your source tree
COPY ./src ./src

# Build for release.
# RUN rm ./target/release/deps/omnisat-indexer-rs*
RUN cargo build --release

################
# Runner
################

# Our Second stage, that will be the final image
FROM scratch

# copy the build artifact from the build stage
COPY --from=builder /usr/src/omnisat-indexer-rs/target/release/omnisat-indexer-rs .

# set the startup command to run your binary
CMD ["./omnisat-indexer-rs"]
