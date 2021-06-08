# Echo server

HTTP echo server, that's it.

[![.github/workflows/build.yml](https://github.com/Swaagie/echo-server/actions/workflows/build.yml/badge.svg)](https://github.com/Swaagie/echo-server/actions/workflows/build.yml)

## Installation

```console
cargo install echo-server
```

## Usage

The HTTP server listens to `port: 8080` by default.

```console
echo-server [8080]
```

> All HTTP verbs are supported.

#### `GET` request

```console
curl -X GET localhost:8080
```

#### `POST` request

```console
curl -X POST -H "Content-Type: application/json" -d '{"hello": "world"}' localhost:8080
```

## Docker

TODO

## Contributing

Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

Please make sure to update tests as appropriate.

## License

[MIT](https://choosealicense.com/licenses/mit/)