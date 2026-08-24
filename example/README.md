# Echo Server Example

A ready-to-run Docker Compose setup that serves **HTTP/2 over TLS** and
**HTTP/3 over QUIC** on the same port, using a self-signed certificate
generated for you on first start.

## Quick Start (Docker Compose)

```console
cd example
docker compose up --build
```

That starts two services: `certs` generates a self-signed certificate for
`localhost` into a shared volume and exits, then `echo-server` starts with both
listeners on port 8443 (TCP for h2, UDP for h3).

You should see:

```
Listening for HTTP/2 over TLS (h2) on 0.0.0.0:8443
Listening for HTTP/3 over QUIC (h3) on 0.0.0.0:8443
```

### Install a client

The examples below use [`xh`](https://github.com/ducaale/xh), a Rust HTTP client
with a friendlier syntax than curl. **HTTP/3 is behind a cargo feature that the
prebuilt packages do not enable**, so which install you want depends on what you
plan to test.

HTTP/2 only (fast, prebuilt):

```console
brew install xh          # or: cargo install xh --locked
```

HTTP/2 **and** HTTP/3 (builds from source, ~1 minute):

```console
RUSTFLAGS='--cfg reqwest_unstable' cargo install xh --features http3 --locked --force
```

The `reqwest_unstable` flag is required: HTTP/3 support in the underlying
`reqwest` crate is still gated as unstable. `--force` makes cargo rebuild even
when some version of `xh` is already installed.

A binary built without the feature accepts the flag but refuses at runtime:

```
xh: error: This binary was built without support for HTTP/3. Enable the `http3` feature.
```

**If you still see that error after installing**, you are running a different
`xh` than the one you just built — usually a packaged copy that your shell
cached or that sits earlier in `PATH`:

```console
hash -r          # zsh/bash: forget the cached path (zsh also accepts `rehash`)
which -a xh      # every xh on PATH, in resolution order — the first one wins
```

`cargo install` puts the binary in `~/.cargo/bin`. If a packaged `xh` (for
example `/opt/homebrew/bin/xh`) resolves first, either put `~/.cargo/bin`
earlier in `PATH`, remove the packaged copy (`brew uninstall xh`), or call the
built one by its full path.

### Talk to it

The certificate is self-signed, so every request below passes `--verify=no`.

**HTTP/2 over TLS:**

```console
xh --verify=no --http-version 2 https://localhost:8443/
```

```
HTTP/2.0 200 OK
user-agent: xh/0.26.2
accept: application/json, */*;q=0.5
content-length: 147
alt-svc: h3=":8443"; ma=3600
```

The `alt-svc` header is the server advertising its HTTP/3 endpoint; that is how
an HTTP/3-capable client discovers the QUIC listener.

**HTTP/3 over QUIC** (needs the `http3` build above):

```console
xh --verify=no --http-version 3-prior-knowledge https://localhost:8443/
```

```
HTTP/3.0 200 OK
```

**Echo a request body** — `xh` builds JSON from `key=value` pairs:

```console
xh --verify=no --http-version 2 POST https://localhost:8443/ hello=world
```

The response is the request headers, a blank line, then the request body:

```
content-type: application/json
content-length: 17

{"hello":"world"}
```

**Send custom headers** with `name:value`:

```console
xh --verify=no --http-version 2 https://localhost:8443/ x-demo:hello
```

Every request header comes back both in the response body and as a response
header, which is the whole point of the server.

> Prefer curl? `curl -k --http2 -i https://localhost:8443/` works for HTTP/2.
> Most curl builds (including the one shipped with macOS) lack HTTP/3.

### Stopping

```console
docker compose down          # keep the generated certificate
docker compose down -v       # also discard it, so the next start makes a new one
```

## Trying mTLS

Set `require_client_certs = true` in `config.toml` and point `ca_cert` at the CA
that signed your client certificate. Both files must be readable inside the
container, so mount them alongside the config:

```yaml
volumes:
  - certs:/certs:ro
  - ./config.toml:/server/config.toml:ro
  - /path/to/your/ca:/ca:ro
```

Then pass the client certificate:

```console
xh --verify=no --http-version 2 https://localhost:8443/ \
  --cert /path/to/client.crt \
  --cert-key /path/to/client.key
```

## Running without Docker

1. Copy the example config file:
   ```console
   cp example/config.toml config.toml
   ```

2. Edit `config.toml` with your settings (port, TLS certificates, etc.). The
   example points at `/certs/...`, which only exists inside the container.

3. Build with HTTP/3 support and run:
   ```console
   cargo build --release --features http3
   ./target/release/echo-server --config config.toml
   ```

   Enable debug logging to see each request:
   ```console
   RUST_LOG=debug echo-server --config config.toml
   ```

## Configuration Options

### Port
Set the port the server listens on (default: 8080):
```toml
port = 8080
```

### Protocol
Select the listener(s) to start (default: `auto`):
```toml
protocol = "auto"   # "h2c", "h2", "h3", or "auto"
```

- `h2c` — HTTP/2 cleartext, no TLS
- `h2` — HTTP/2 over TLS, requires a `[tls]` section
- `h3` — HTTP/3 over QUIC, requires a `[tls]` section and `--features http3`
- `auto` — with TLS, serves h2 and (when built with `--features http3`) h3 on
  the same port; without TLS, serves h2c

HTTP/3 is behind a cargo feature. A binary built without it exits with an error
if you ask for `h3`, and `auto` serves h2 only.

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

## Testing with xh

All examples assume `xh` is installed (see [Install a client](#install-a-client))
and use `--verify=no` because the example certificate is self-signed. Drop that
flag once you use a certificate your machine trusts.

### HTTP/2 cleartext (h2c)

No TLS, so the server must be running with `protocol = "h2c"`:

```console
xh --http-version 2-prior-knowledge http://localhost:8080/
```

### HTTP/2 over TLS (h2)

```console
# Without client certificates
xh --verify=no --http-version 2 https://localhost:8443/

# With mTLS
xh --verify=no --http-version 2 https://localhost:8443/ \
  --cert /path/to/client.crt \
  --cert-key /path/to/client.key
```

### HTTP/3 over QUIC (h3)

Requires an `xh` built with the `http3` feature:

```console
xh --verify=no --http-version 3-prior-knowledge https://localhost:8443/
```

### Request bodies and headers

```console
# JSON body from key=value pairs
xh --verify=no --http-version 2 POST https://localhost:8443/ name=echo count:=3

# Raw body
xh --verify=no --http-version 2 PUT https://localhost:8443/ --raw 'plain text'

# Custom headers use name:value
xh --verify=no --http-version 2 https://localhost:8443/ x-request-id:abc123
```

`GET`, `POST`, `PUT` and `PATCH` on `/` echo the request; `OPTIONS` returns an
empty 200; anything else returns 404.

### Notes

- `--http-version 2-prior-knowledge` skips the HTTP/1.1 upgrade for cleartext
  HTTP/2; plain `2` is correct over TLS, where ALPN negotiates the protocol
- `--verify=no` skips certificate verification; use it only with throwaway
  self-signed certificates
- Add `-v` to see the full request and response, or `-h` for response headers only
- If a hostname must match the certificate, `--resolve host:127.0.0.1` points it
  at your local server

## Running with Docker directly

The Compose setup above is the easiest path. To run the image by hand, build it
with the HTTP/3 feature and mount both the config and the certificates:

```console
docker build --build-arg FEATURES=http3 -t echo-server ..
docker run --rm \
  -p 8443:8443/tcp -p 8443:8443/udp \
  -v $(pwd)/config.toml:/server/config.toml:ro \
  -v $(pwd)/certs:/certs:ro \
  -e RUST_LOG=info \
  --name echo echo-server \
  --config /server/config.toml
```

Certificate paths in the config are resolved relative to the config file's
directory, so absolute paths (as in the example config) are the least
surprising choice. Publish the UDP port as well as the TCP one, or HTTP/3 will
be unreachable even though the server is listening.
