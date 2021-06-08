use std::env;
use std::net::SocketAddr;
use std::convert::Infallible;

use hyper::{Body, Request, Response, Server, Method, StatusCode};
use hyper::service::{make_service_fn, service_fn};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let port = get_port(&args);

    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let server = Server::bind(&address).serve(
        make_service_fn(|_server| async {
            Ok::<_, Infallible>(service_fn(handle_request))
        })
    );

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
        },
        (&Method::POST, "/") => {
            *response.body_mut() = req.into_body();
        },
        (&Method::PUT, "/") => {
            *response.body_mut() = req.into_body();
        },
        (&Method::PATCH, "/") => {
            *response.body_mut() = req.into_body();
        },
        (&Method::OPTIONS, "/") => {
            *response.status_mut() = StatusCode::OK;
        },
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        },
    };

    Ok(response)
}

// #[test]
// fn test_parse_parameters() -> Result<(), std::string::FromUtf8Error> {
//     let args = vec![
//         String::from("exec"),
//         String::from("--port=80"),
//         String::from("--body=hello"),
//     ];
//     let (port, body) = parse_parameters(&args);

//     assert_eq!(port, "80");
//     assert_eq!(body, "hello");

//     // Single parameter
//     let args = vec![String::from("exec"), String::from("--port=8081")];
//     let (port, body) = parse_parameters(&args);

//     assert_eq!(port, "8081");
//     assert_eq!(body, "hello world");

//     // Invert order
//     let args = vec![
//         String::from("exec"),
//         String::from("--body=first"),
//         String::from("--port=8082"),
//     ];
//     let (port, body) = parse_parameters(&args);

//     assert_eq!(port, "8082");
//     assert_eq!(body, "first");

//     Ok(())
// }

// #[test]
// fn test_parse_parameters_with_defaults() -> Result<(), std::string::FromUtf8Error> {
//     let args = vec![String::from("exec"), String::from(""), String::from("")];
//     let (port, body) = parse_parameters(&args);

//     assert_eq!(port, "8080");
//     assert_eq!(body, "hello world");

//     Ok(())
// }
