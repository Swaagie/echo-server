# Echo server

HTTP echo server, that's it.

[![.github/workflows/build.yaml](https://github.com/Swaagie/echo-server/actions/workflows/build.yaml/badge.svg)](https://github.com/Swaagie/echo-server/actions/workflows/build.yaml)

## Installation

```console
cargo install echo-server
```

## Usage

Defaults:
- HTTP server listens to `0.0.0.0:8080`.
- HTTP request headers return with the response.
- GET requests have no body content.

```console
echo-server [--port=8080] [--body="Custom GET response body"]
```

> All HTTP verbs are supported.

#### `GET` request

```console
curl -vvv -X GET localhost:8080
curl -vvv -X GET -H "x-random-header: test" localhost:8080
```

#### `POST` request

```console
curl -vvv -X POST -H "Content-Type: application/json" -d '{"hello": "world"}' localhost:8080
```

## Docker

You can run a precompiled image from Docker hub:

```console
docker run --rm -p 8080:8080 --name echo swaagie/echo-server:latest
```

Or build the image local:

```console
docker build -t echo-server .
docker run --rm -p 8080:8080 --name echo echo-server
```

Listen on a different port:

```console
docker run --rm -p 8081:8081 --name echo echo-server --port=8081
```

## Contributing

Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

Please make sure to update tests as appropriate.

## License

[MIT]

[MIT]: https://choosealicense.com/licenses/mit/
[hub]: https://hub.docker.com/repository/docker/swaagie/echo-server