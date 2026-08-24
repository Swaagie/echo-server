# Echo server

HTTP/2 and HTTP/3 echo server that echoes request headers and body.

[![.github/workflows/build.yaml](https://github.com/Swaagie/echo-server/actions/workflows/build.yaml/badge.svg)](https://github.com/Swaagie/echo-server/actions/workflows/build.yaml)

## Features

- **HTTP/2 cleartext (h2c)** - HTTP/2 without TLS
- **HTTP/2 over TLS (h2)** - Secure HTTP/2 with optional mTLS
- **HTTP/3 over QUIC (h3)** - Modern HTTP/3 protocol (requires `--features http3`)
- **Automatic protocol selection** - Serve h2 and h3 simultaneously on the same port
- **mTLS support** - Mutual TLS with client certificate verification

## Installation

### From source

```console
cargo install echo-server
```

### With HTTP/3 support

```console
cargo install echo-server --features http3
```

### Building locally

```console
git clone https://github.com/Swaagie/echo-server.git
cd echo-server

# Standard build (HTTP/2 only)
cargo build --release

# With HTTP/3 support
cargo build --release --features http3
```

## Usage

```console
echo-server [-p|--port=8080] [--config=config.toml]
```

**Logging:**

Enable debug logging with the `RUST_LOG` environment variable:

```console
RUST_LOG=debug echo-server

# Or for just this crate:
RUST_LOG=echo_server=debug echo-server
```

## Configuration

### CLI flags

- `-p, --port <port>`: Port to listen on (default: 8080)
- `--config <path>`: Path to TOML config file (default: config.toml)

### Configuration file

```toml
# Port to listen on (optional, defaults to 8080)
port = 8080

# Protocol selection: "h2c", "h2", "h3", or "auto" (default)
# - h2c: HTTP/2 cleartext (no TLS required)
# - h2: HTTP/2 over TLS (requires TLS)
# - h3: HTTP/3 over QUIC (requires TLS, needs --features http3)
# - auto: Automatically select based on TLS configuration
#   - With TLS: Start both H2 (TCP) and H3 (UDP) on same port
#   - Without TLS: Start H2c (TCP) only
protocol = "auto"

# TLS/mTLS configuration
[tls]
server_cert = "/path/to/server.crt"
server_key = "/path/to/server.key"

# CA certificate for client verification (required if require_client_certs is true)
ca_cert = "/path/to/ca.crt"

# Require and validate client certificates (mTLS). Default: false
require_client_certs = true
```

See [`example/README.md`](example/README.md) for more configuration details and curl examples.

## Running with Docker Compose

The `example/` directory contains a ready-to-use Docker Compose setup:

```console
cd example
docker compose up --build
```

This generates a self-signed certificate and serves **h2 (TCP)** and **h3 (UDP)** on port 8443:

```console
xh --verify=no --http-version 2 https://localhost:8443/
```

See [`example/README.md`](example/README.md) for HTTP/3 clients, mTLS, and running the image directly.

## Testing with curl

### HTTP/2 Cleartext (h2c)

```bash
curl -v --http2 http://localhost:8080 --http2-prior-knowledge
```

### HTTP/2 with TLS (h2)

```bash
# Without client certificates
curl -v --http2 https://localhost:8443 -k

# With mTLS (client certificates)
curl -v --http2 https://localhost:8443 \
  --cert /path/to/client.crt \
  --key /path/to/client.key \
  -k
```

### HTTP/3 with QUIC (h3)

Requires curl 7.88.0+ compiled with HTTP/3 support:

```bash
curl -v --http3 https://localhost:8443 -k
```

### POST request with body

```bash
curl -v --http2 http://localhost:8080 --http2-prior-knowledge \
  -H 'Content-Type: application/json' \
  -d '{"test": "data"}'
```

## Protocol Comparison

| Protocol | TLS Required | Transport | Feature Flag |
|----------|--------------|-----------|--------------|
| h2c      | No           | TCP       | (default)    |
| h2       | Yes          | TCP       | (default)    |
| h3       | Yes          | UDP/QUIC  | `http3`      |
| auto     | Optional     | TCP + UDP | `http3`      |

## License

[MIT](https://choosealicense.com/licenses/mit/)
