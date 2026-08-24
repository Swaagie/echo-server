FROM rust:alpine AS builder

# cmake/clang/perl are needed to build aws-lc-sys against musl.
RUN apk add --no-cache musl-dev cmake make clang clang-dev llvm-dev perl

# e.g. FEATURES=http3 to build the QUIC listener.
ARG FEATURES=""

RUN mkdir /server
WORKDIR /server
COPY . .

RUN if [ -n "$FEATURES" ]; then \
      cargo build --release --features "$FEATURES"; \
    else \
      cargo build --release; \
    fi

FROM alpine:latest AS final

ENV USER="app"

RUN addgroup -S $USER
RUN adduser -S -g $USER $USER

RUN mkdir /server
WORKDIR /server

COPY --from=builder /server/target/release/echo-server /server/echo-server
RUN chown -R $USER:$USER /server

USER $USER

EXPOSE 8080

ENTRYPOINT ["/server/echo-server"]
