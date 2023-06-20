FROM rust

WORKDIR /usr/src/myapp
COPY src ./src
COPY Cargo.toml .
COPY entrypoint.sh .
CMD ["entrypoint.sh"]
# CMD sleep 30000