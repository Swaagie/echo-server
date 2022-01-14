use std::convert::Infallible;
use std::env;
use std::net::SocketAddr;
use std::num::ParseIntError;

use hyper::service::{make_service_fn, service_fn};
use hyper::header::{HeaderValue, CONTENT_LENGTH};
use hyper::{Body, Method, Request, Response, Server, StatusCode};

#[derive(PartialEq, Debug)]
struct ParseError {}

#[derive(PartialEq, Debug)]
enum InvalidArgument {
    Parse(ParseError),
    ParseInt(ParseIntError)
}

impl From<ParseIntError> for InvalidArgument {
    fn from(err: ParseIntError) -> Self {
        InvalidArgument::ParseInt(err)
    }
}

#[tokio::main]
async fn main() {
    let port = get_port(&env::args().collect::<Vec<String>>()).unwrap_or(8080);
    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let server = Server::bind(&address).serve(make_service_fn(|_server| async {
        Ok::<_, Infallible>(service_fn(handle_request))
    }));

    // Allow server to be killed.
    let server = server.with_graceful_shutdown(async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to add signal handler")
    });

    println!("Echo server listening on port {}", port);
    if let Err(e) = server.await {
        eprintln!("Server error: {}", e);
    }
}

fn get_argument(name: &str, args: &[String]) -> Option<String> {
    match args.iter().find(|arg| arg.contains(name)) {
        Some(arg) => arg.split('=').map(|v| v.to_owned()).collect::<Vec<String>>().pop(),
        _ => None
    }
}

fn get_port(args: &[String]) -> Result<u16, InvalidArgument> {
    let port = get_argument("--port", args);

    match port {
        Some(p) => Ok(p.parse::<u16>()?),
        None => Err(InvalidArgument::Parse(ParseError {}))
    }
}

fn get_body(args: &[String]) -> Result<String, InvalidArgument> {
    get_argument("--body", args)
        .ok_or(InvalidArgument::Parse(ParseError {}))
}

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let mut response = Response::new(Body::empty());
    let echo_headers = response.headers_mut();
    let headers = req.headers();

    // Echo HTTP headers`
    headers.iter().for_each(|(name, value)| {
        echo_headers.insert(name, value.clone());
    });

    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => {
            let body = get_body(&env::args().collect::<Vec<String>>()).unwrap_or_else(|_| "".to_owned());
            let content_length = HeaderValue::from_str(
                &body.as_bytes().len().to_string()
            ).unwrap_or_else(|_| HeaderValue::from_static("0"));

            echo_headers.insert(CONTENT_LENGTH, content_length);
            *response.body_mut() = Body::from(body);
        }
        (&Method::POST, "/") => {
            *response.body_mut() = req.into_body();
        }
        (&Method::PUT, "/") => {
            *response.body_mut() = req.into_body();
        }
        (&Method::PATCH, "/") => {
            *response.body_mut() = req.into_body();
        }
        (&Method::OPTIONS, "/") => {
            *response.status_mut() = StatusCode::OK;
        }
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        }
    };

    Ok(response)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_get_body() {
        let args = ["--body=hello world", "body=bad", "--body=42"].map(|v| v.to_string());

        assert_eq!(get_body(&args[..]), Ok("hello world".to_owned()));
        assert_eq!(get_body(&args[1..]), Ok("42".to_owned()));
    }

    #[test]
    fn test_get_port() {
        let args = ["--port=1337", "--port=8081"].map(|v| v.to_string());

        assert_eq!(get_port(&args[..]), Ok(1337));
        assert_eq!(get_port(&args[1..]), Ok(8081));
    }

    #[test]
    fn test_get_argument() {
        let args = ["--port=8080", "port=8081"].map(|v| v.to_string());
        assert_eq!(get_argument("--port", &args[..]), Some("8080".to_string()));
    }
}