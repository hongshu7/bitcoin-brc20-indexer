FROM rust

WORKDIR /usr/src/myapp
COPY src ./src
COPY Cargo.toml .
COPY entrypoint.sh /usr/local/bin/
RUN chmod +x /usr/local/bin/entrypoint.sh
CMD ["entrypoint.sh"]
# CMD sleep 30000