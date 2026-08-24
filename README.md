# Echo server

HTTP/2 and HTTP/3 echo server that echoes request headers and body.

[![Rust CICD](https://github.com/Swaagie/echo-server/actions/workflows/ci.yaml/badge.svg)](https://github.com/Swaagie/echo-server/actions/workflows/ci.yaml)
[![Docker image build](https://github.com/Swaagie/echo-server/actions/workflows/build.yaml/badge.svg)](https://github.com/Swaagie/echo-server/actions/workflows/build.yaml)

## Install

```console
cargo install echo-server                  # HTTP/2 only
cargo install echo-server --features http3 # adds HTTP/3 over QUIC
```

The `http3` feature is available from 4.0.0. Releases up to 3.0.0 serve
HTTP/1.1 instead and have no feature flags.

## Usage

```console
echo-server [-p|--port <port>] [--config <path>] [--protocol <h2c|h2|h3|auto>]
```

| Flag | Default | Description |
|---|---|---|
| `-p, --port` | `8080` | Port to listen on |
| `--config` | `config.toml` | Path to the TOML config file |
| `--protocol` | `auto` | `h2c`, `h2`, `h3`, or `auto` |

TLS can only be configured through the config file. Set `RUST_LOG=debug` to log
every request.

## Protocols

| Protocol | TLS | Transport | Requires `http3` feature |
|----------|-----|-----------|--------------------------|
| `h2c`    | No       | TCP      | No  |
| `h2`     | Required | TCP      | No  |
| `h3`     | Required | UDP/QUIC | Yes |
| `auto`   | Optional | TCP + UDP | Only for the HTTP/3 listener |

`auto` serves h2c without TLS, and with TLS serves h2 plus — when built with
`--features http3` — h3 on the same port, advertised to HTTP/2 clients through
an `Alt-Svc` header. Without the feature, `auto` serves h2 only and `h3` exits
with an error.

## Endpoints

`GET`, `HEAD`, `POST`, `PUT` and `PATCH` on `/` return the request headers, and
for methods with a body a blank line followed by that body. `OPTIONS /` returns
an empty `200`. Every other path returns `404`. Request headers are echoed back
as response headers.

## Configuration file

```toml
port = 8080
protocol = "auto"

[tls]
server_cert = "/path/to/server.crt"
server_key = "/path/to/server.key"

# Required when require_client_certs is true
ca_cert = "/path/to/ca.crt"
require_client_certs = false
```

Relative certificate paths resolve against the config file's directory.

## Docker Compose

```console
cd example
docker compose up --build
```

Generates a self-signed certificate and serves h2 (TCP) and h3 (UDP) on 8443:

```console
xh --verify=no --http-version 2 https://localhost:8443/
```

See [`example/README.md`](example/README.md) for HTTP/3 clients, mTLS, and
running the image directly.

## Releasing

Publishing to crates.io runs on a version tag, after the tests pass:

```console
git tag v4.0.1
git push origin v4.0.1
```

The tag must match `version` in `Cargo.toml`, or the workflow stops before
publishing.

## License

[MIT](https://choosealicense.com/licenses/mit/)
