# Build image
FROM rust:alpine AS builder

# musl-dev for the C toolchain; cmake/clang/perl are required to build
# aws-lc-sys (the rustls crypto provider) against musl.
RUN apk add --no-cache musl-dev cmake make clang clang-dev llvm-dev perl

# Optional cargo features, e.g. FEATURES=http3 to build the QUIC listener.
ARG FEATURES=""

RUN mkdir /server
WORKDIR /server
COPY . .

RUN if [ -n "$FEATURES" ]; then \
      cargo build --release --features "$FEATURES"; \
    else \
      cargo build --release; \
    fi

# Final image
FROM alpine:latest AS final

ENV USER="app"

# Define user that executes the echo-server
RUN addgroup -S $USER
RUN adduser -S -g $USER $USER

RUN mkdir /server
WORKDIR /server

COPY --from=builder /server/target/release/echo-server /server/echo-server
RUN chown -R $USER:$USER /server

USER $USER

# Expose default port of echo-server
EXPOSE 8080

ENTRYPOINT ["/server/echo-server"]
