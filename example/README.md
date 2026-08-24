# Echo Server Example

Docker Compose setup serving **HTTP/2 over TLS** and **HTTP/3 over QUIC** on the
same port, with a self-signed certificate generated on first start.

## Run it

```console
cd example
docker compose up --build
```

```
Listening for HTTP/2 over TLS (h2) on 0.0.0.0:8443
Listening for HTTP/3 over QUIC (h3) on 0.0.0.0:8443
```

Stop with `docker compose down`, or `down -v` to also discard the certificate.

## Install a client

Examples use [`xh`](https://github.com/ducaale/xh). HTTP/3 is behind a cargo
feature the prebuilt packages do not enable, so pick the install you need:

```console
brew install xh                                                                  # HTTP/2 only
RUSTFLAGS='--cfg reqwest_unstable' cargo install xh --features http3 --locked --force  # adds HTTP/3
```

A binary built without the feature accepts `--http-version 3-prior-knowledge`
and then refuses:

```
xh: error: This binary was built without support for HTTP/3. Enable the `http3` feature.
```

If that persists after installing, a packaged `xh` earlier in `PATH` is
answering instead of the one in `~/.cargo/bin`. Run `hash -r` to clear the
shell's cached path, and `which -a xh` to see the resolution order.

## Requests

The certificate is self-signed, so every request passes `--verify=no`.

```console
# HTTP/2 over TLS
xh --verify=no --http-version 2 https://localhost:8443/

# HTTP/3 over QUIC
xh --verify=no --http-version 3-prior-knowledge https://localhost:8443/

# JSON body from key=value pairs, raw body, and custom headers
xh --verify=no --http-version 2 POST https://localhost:8443/ name=echo count:=3
xh --verify=no --http-version 2 PUT https://localhost:8443/ --raw 'plain text'
xh --verify=no --http-version 2 https://localhost:8443/ x-request-id:abc123
```

Responses carry `alt-svc: h3=":8443"`, which is how an HTTP/3-capable client
discovers the QUIC listener. `curl -k --http2 https://localhost:8443/` also
works; most curl builds lack HTTP/3.

## mTLS

Set `require_client_certs = true` in `config.toml`, point `ca_cert` at the
signing CA, and mount it into the container:

```yaml
volumes:
  - certs:/certs:ro
  - ./config.toml:/server/config.toml:ro
  - /path/to/your/ca:/ca:ro
```

```console
xh --verify=no --http-version 2 https://localhost:8443/ \
  --cert /path/to/client.crt --cert-key /path/to/client.key
```

## Without Compose

```console
docker build --build-arg FEATURES=http3 -t echo-server ..
docker run --rm -p 8443:8443/tcp -p 8443:8443/udp \
  -v $(pwd)/config.toml:/server/config.toml:ro \
  -v $(pwd)/certs:/certs:ro \
  echo-server --config /server/config.toml
```

Publish the UDP port as well as TCP, or HTTP/3 is unreachable even though the
server is listening.

Running the binary directly needs `cargo build --release --features http3` and a
config pointing at certificates that exist on the host.
