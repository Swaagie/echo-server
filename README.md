# Echo server

HTTP2 echo server that echoes request headers and body.

[![.github/workflows/build.yaml](https://github.com/Swaagie/echo-server/actions/workflows/build.yaml/badge.svg)](https://github.com/Swaagie/echo-server/actions/workflows/build.yaml)

## Installation

```console
cargo install echo-server
```

## Usage

```console
echo-server [-p|--port=8080] [--config=config.toml]
```

**Behavior:**
- GET: Returns request headers as response body
- POST/PUT/PATCH: Returns request headers + blank line + request body

**Logging:**
Enable debug logging with the `RUST_LOG` environment variable:
```console
RUST_LOG=debug echo-server
# Or for just this crate:
RUST_LOG=echo_server=debug echo-server
```

**Example:**
```console
curl -X GET -H "x-test: value" localhost:8080
# Response body: headers formatted as "header-name: value"
```

## Configuration

See [`example/README.md`](example/README.md) for configuration details and examples.

**Quick start:**
```console
cp example/config.toml config.toml
echo-server
```

**CLI flags:**
- `-p, --port <port>`: Port to listen on (default: 8080)
- `--config <path>`: Path to TOML config file (default: config.toml)

**Config file options:**
- `port`: Port number (optional, defaults to 8080)
- `[tls]`: TLS/mTLS configuration (server_cert, server_key, ca_cert)

## Docker

```console
docker run --rm -p 8080:8080 swaagie/echo-server:latest
```

See [`example/README.md`](example/README.md) for Docker examples with TLS.

## License

[MIT](https://choosealicense.com/licenses/mit/)
