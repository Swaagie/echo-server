use std::convert::Infallible;
use std::env;
use std::net::SocketAddr;

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let port = get_port(&args);

    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let server = Server::bind(&address).serve(make_service_fn(|_server| async {
        Ok::<_, Infallible>(service_fn(handle_request))
    }));

    println!("Echo server listening on port {}", port);
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

fn get_port(args: &[String]) -> u16 {
    let mut port = 8080;

    if args.len() == 2 {
        port = args[1].parse::<u16>().expect("Provide valid port");
    }

    port
}

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let mut response = Response::new(Body::empty());

    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => {
            *response.body_mut() = Body::from("Use {POST, PUT, PATCH} to echo");
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
fn test_get_port() -> Result<(), std::string::FromUtf8Error> {
    let args = vec![String::from("exec"), String::from("80")];

    assert_eq!(get_port(&args), 80);

    Ok(())
}

#[test]
fn test_get_port_with_defaults() -> Result<(), std::string::FromUtf8Error> {
    let args = vec![String::from("exec")];

    assert_eq!(get_port(&args), 8080);

    Ok(())
}
