# Echo Server Example Configuration

This directory contains an example configuration file for the echo server.

## Quick Start

1. Copy the example config file:
   ```console
   cp example/config.toml config.toml
   ```

2. Edit `config.toml` with your settings (port, TLS certificates, etc.)

3. Run the server:
   ```console
   echo-server
   ```

   Or specify a custom config path:
   ```console
   echo-server --config /path/to/config.toml
   ```

   Enable debug logging to see each request:
   ```console
   RUST_LOG=debug echo-server
   ```

## Configuration Options

### Port
Set the port the server listens on (default: 8080):
```toml
port = 8080
```

### TLS/mTLS
To enable mutual TLS, provide all three certificates:
```toml
[tls]
server_cert = "/path/to/server.crt"
server_key = "/path/to/server.key"
ca_cert = "/path/to/ca.crt"
require_client_certs = true
```

All certificates must be in PEM format. When `require_client_certs` is `true`, the server will require client certificates signed by the provided CA.

## Testing with curl

### HTTP/2 Cleartext (h2c)

HTTP/2 cleartext without TLS:

```bash
curl -v --http2 http://localhost:8080 --http2-prior-knowledge
```

### HTTP/2 with TLS (h2)

**Without mTLS** (client certificates not required):

```bash
curl -v --http2 https://example.com:8080 --http2-prior-knowledge \
  --connect-to example.com:8080:localhost:8080
```

**With mTLS** (client certificates required):

```bash
curl -v --http2 https://example.com:8080 --http2-prior-knowledge \
  --connect-to example.com:8080:localhost:8080 \
  --cert /path/to/example.com_client.crt \
  --key /path/to/example.com_client.key
```

### HTTP/3 with QUIC (h3)

**Note**: HTTP/3 support requires curl 7.88.0+ compiled with HTTP/3 support.

**Without mTLS** (client certificates not required):

```bash
curl -v --http3 https://example.com:8080 \
  --connect-to example.com:8080:localhost:8080
```

**With mTLS** (client certificates required):

```bash
curl -v --http3 https://example.com:8080 \
  --connect-to example.com:8080:localhost:8080 \
  --cert /path/to/example.com_client.crt \
  --key /path/to/example.com_client.key
```

### POST/PUT/PATCH Requests

To test with a request body:

```bash
# POST request
curl -v --http2 https://example.com:8080 --http2-prior-knowledge \
  --connect-to example.com:8080:localhost:8080 \
  --cert /path/to/example.com_client.crt \
  --key /path/to/example.com_client.key \
  -H 'Content-Type: application/json' \
  -d '{"test": "data"}'

# PUT request
curl -v --http2 https://example.com:8080 --http2-prior-knowledge \
  --connect-to example.com:8080:localhost:8080 \
  --cert /path/to/example.com_client.crt \
  --key /path/to/example.com_client.key \
  -X PUT \
  -d 'test=value'
```

### Notes

- Use `--http2-prior-knowledge` for HTTP/2 to skip the HTTP/1.1 upgrade negotiation
- Use `--connect-to example.com:8080:localhost:8080` to connect to localhost while using `example.com` in the URL (useful for testing with certificates that match a specific hostname)
- Replace certificate paths with your actual certificate file paths
- Replace `example.com:8080` with your actual hostname and port if different

## Running with Docker

### Using Docker Compose (Recommended)

The easiest way to run the echo server is using Docker Compose:

**Important**: If your `config.toml` references certificate files or other files (e.g., `server_cert = "./certs/server.crt"`), you must also mount those files/directories in the `docker-compose.yml` volumes section. The paths in the config file will be resolved relative to the config file's directory inside the container.

```console
cd example
docker-compose up -d
```

Or with a custom port:

```console
docker-compose up -d
docker-compose exec echo-server echo-server --port 9000
```

**Example**: If your config uses relative paths for certificates, update `docker-compose.yml` to mount the certificates directory:

```yaml
volumes:
  - ./config.toml:/server/config.toml:ro
  - ./certs:/server/certs:ro  # Mount certificates directory
```

### Using Docker directly

**Important**: If your `config.toml` references certificate files or other files (e.g., `server_cert = "./certs/server.crt"`), you must also mount those files/directories into the container. The paths in the config file will be resolved relative to the config file's directory inside the container.

```console
docker run --rm -p 8080:8080 \
  -v $(pwd)/config.toml:/server/config.toml:ro \
  --name echo echo-server \
  --config /server/config.toml
```

With debug logging:
```console
docker run --rm -p 8080:8080 \
  -e RUST_LOG=debug \
  -v $(pwd)/config.toml:/server/config.toml:ro \
  --name echo echo-server \
  --config /server/config.toml
```

**Example with certificates**: If your config uses relative paths like `server_cert = "./certs/server.crt"`, mount the certificates directory:

```console
docker run --rm -p 8080:8080 \
  -v $(pwd)/config.toml:/server/config.toml:ro \
  -v $(pwd)/certs:/server/certs:ro \
  --name echo echo-server \
  --config /server/config.toml
```

### Running with mTLS

Update `config.toml` with your certificate paths, then mount the certificates directory:

```console
docker run --rm -p 8443:8443 \
  -v $(pwd)/config.toml:/server/config.toml:ro \
  -v /path/to/certs:/certs:ro \
  --name echo echo-server \
  --port=8443 \
  --config=/server/config.toml
```

Or use the docker-compose.yml file and uncomment the mTLS service section.

