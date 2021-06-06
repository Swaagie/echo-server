# Build image
FROM rust:alpine as builder

RUN mkdir /server
WORKDIR /server
COPY . .

RUN cargo build --release

# Final image
FROM alpine:latest

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