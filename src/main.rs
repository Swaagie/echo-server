use std::convert::Infallible;
use std::env;
use std::net::SocketAddr;

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};

#[tokio::main]
async fn main() {
    let port = get_port();
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
        eprintln!("server error: {}", e);
    }
}

fn get_argument(args: &[String], name: &str) -> Option<String> {
    for arg in args.iter() {
        let arg: Vec<&str> = arg.split('=').collect();

        if arg[0] == name {
            return Some(arg[1].to_owned());
        }
    }

    None
}

fn get_port() -> u16 {
    let args: Vec<String> = env::args().collect();

    match get_argument(&args, "--port") {
        Some(p) => p.parse::<u16>().expect("Provide valid port"),
        None => 8080,
    }
}

fn get_body() -> String {
    let args: Vec<String> = env::args().collect();

    match get_argument(&args, "--body") {
        Some(s) => s,
        None => String::from("Use {POST, PUT, PATCH} to echo"),
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

    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => {
            *response.body_mut() = Body::from(get_body());
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

#[test]
fn test_get_body() -> Result<(), std::string::FromUtf8Error> {
    assert_eq!(get_body(), "Use {POST, PUT, PATCH} to echo");

    Ok(())
}

#[test]
fn test_get_port() -> Result<(), std::string::FromUtf8Error> {
    assert_eq!(get_port(), 8080);

    Ok(())
}
