# Changelog

## 4.0.0

### Breaking

- HTTP/1.1 is no longer served. The server speaks HTTP/2 (`h2c`, `h2`) and,
  with the `http3` feature, HTTP/3 over QUIC.
- The `--body` and `--header` flags are gone. Responses echo the request.
- TLS is configured through the config file only.

### Added

- HTTP/2 over TLS and HTTP/3 over QUIC, with mutual TLS.
- `--protocol` to select `h2c`, `h2`, `h3`, or `auto`.
- `auto` serves h2 and h3 on the same port and advertises h3 via `Alt-Svc`.
- `HEAD /` mirrors `GET /`.
- End-to-end tests that drive a real server over real sockets.

### Fixed

- TLS panicked at startup in every `http3` build because two rustls crypto
  providers were compiled in. Pinned to aws-lc-rs, which also enables
  post-quantum key exchange (X25519MLKEM768).
- Responses no longer echo the request's `Content-Length` or hop-by-hop
  headers, so bodyless responses stop advertising a length they never send.
- A truncated request body returns 400 instead of panicking the connection.
- Startup failures are logged as errors rather than silently exiting.
- `auto` exits non-zero when a listener fails instead of serving one transport.
- Shutdown drains in-flight requests.
