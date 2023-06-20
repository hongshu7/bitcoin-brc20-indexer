FROM rust

WORKDIR /usr/src/myapp
COPY src ./src
COPY Cargo.toml .
CMD ["entrypoint.sh"]
# CMD sleep 30000