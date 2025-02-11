use std::convert::Infallible;
use std::net::SocketAddr;
use std::str::FromStr;

use hyper::service::{make_service_fn, service_fn};
use hyper::header::{HeaderName, HeaderValue, CONTENT_LENGTH};
use hyper::{Body, Method, Request, Response, Server, StatusCode};

use structopt::StructOpt;

/// Find acronym meaning.
#[derive(Debug, StructOpt)]
#[structopt(name = "args", about = "Provide echo server configuration")]
struct Cli {
    /// The acronym to search for
    #[structopt(short, long)]
    port: Option<u16>,

    /// Context to search in
    #[structopt(short, long)]
    body: Option<String>,

    /// Headers to add in the response, repeated key:value pairs
    #[structopt(short, long)]
    header: Option<Vec<String>>,
}

#[tokio::main]
async fn main() {
    let args = Cli::from_args();
    let port = args.port.unwrap_or(8080);
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

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let mut response = Response::new(Body::empty());
    let echo_headers = response.headers_mut();
    let headers = req.headers();

    // Echo HTTP headers
    headers.iter().for_each(|(name, value)| {
        echo_headers.insert(name, value.clone());
    });

    // Add static configured response headers
    let args = Cli::from_args();
    if let Some(overwrite_headers) = args.header {
        for key_value in overwrite_headers {
            if let Some(pos) = key_value.find(':') {
                match (HeaderName::from_str(&key_value[..pos]), HeaderValue::from_str(&key_value[pos + 1..])) {
                    (Ok(key), Ok(value)) => {
                        echo_headers.insert(key, value);
                    }
                    _ => {}
                }
            }
        }
    }

    // Handle each HTTP verb
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => {
            let body = args.body.unwrap_or_default();
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
        let resp = tokio_test::block_on(handle_request(Request::new(Body::from("hello world"))));

        assert_eq!(resp.unwrap().status(), StatusCode::OK);
    }
}