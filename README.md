# Echo server

Zero dependency minimalist echo server written in Rust.

## Installation

```console
cargo install echo-server
```

## Usage

```console
echo-server [--port=1337] [--body="your preferred response message"]
```

The server has defaults:

```yaml
- port: 8080
- body: "hello world"
```

> You can also us the build docker image from docker hub.

## Contributing

Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

Please make sure to update tests as appropriate.

## License

[MIT](https://choosealicense.com/licenses/mit/)