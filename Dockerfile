# docker build -t tce-app -f . tce
# docker run --rm -it -v ./workspace:/workspace tce-app

FROM alpine:3.23

RUN apk add --no-cache rust cargo musl-dev bash ncurses libgcc

WORKDIR /app

COPY Cargo.toml Cargo.lock* ./

COPY src ./src

RUN cargo build --release

WORKDIR /workspace

RUN cp /app/target/release/tce /usr/local/bin/tce

CMD ["/bin/bash"]
