FROM rust:slim
ADD . /src
WORKDIR /src
RUN apt-get update && apt-get install -y libmariadb-dev-compat \
  && cargo build --release

FROM debian:stable-slim
RUN apt-get update && apt-get install -y libmariadb-dev-compat && rm -rf /var/lib/apt/lists/*
COPY --from=0 /src/target/release/clacks /bin/clacks

CMD [ "/bin/clacks" ]
