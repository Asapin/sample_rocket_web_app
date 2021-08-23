FROM rust:1.54 as builder

RUN apt update && apt install -y musl-tools musl-dev && \ 
    update-ca-certificates

WORKDIR /app

COPY . .
#RUN RUSTFLAGS=-Clinker=musl-gcc cargo build --release --target=x86_64-unknown-linux-musl
RUN cargo build --release

FROM debian:buster-slim as production

RUN apt update && apt install -y postgresql-client
COPY --from=builder /app/target/release/rest /app/rest
WORKDIR /app

COPY site/static/ ./site/static/
ENV ROCKET_ADDRESS=0.0.0.0
EXPOSE 8000

CMD ["./rest"]
