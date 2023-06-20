FROM rust

WORKDIR /usr/src/myapp
COPY src ./src
COPY Cargo.toml .
RUN echo "RPC_URL="$RPC_URL >> .env
RUN echo "RPC_USER="$RPC_USER >> .env
RUN echo "RPC_PASSWORD="$RPC_PASSWORD >> .env
#RUN cargo run
CMD sleep 30000