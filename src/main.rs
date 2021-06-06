use std::{env, thread};
use std::io::prelude::*;
use std::net::TcpListener;
use std::net::TcpStream;

fn main() -> Result<(), std::io::Error> {
    let args: Vec<String> = env::args().collect();
    let (port, _) = parse_parameters(&args);

    let address = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(address)?;
    println!("Echo server listening on port {}", port);

    for stream in listener.incoming() {
        let stream = stream?;
        let address = stream.peer_addr()?;

        // TODO: Implement max concurrency/threads, potential fork bomb
        thread::spawn(move || -> Result<(), std::io::Error> {
            // TODO: pass body value from main thread.
            let args: Vec<String> = env::args().collect();
            let (_, body) = parse_parameters(&args);

            match handle_connection(stream, body) {
                Ok(_) => println!("Handled request from {}", address),
                Err(err) => println!("Unable to handle request {}", err)
            };

            Ok(())
        });
    }

    println!("{:?}",args);
    Ok(())
}

fn parse_parameters(args: &[String]) -> (&str, &str) {
    let mut port = "8080";
    let mut body = "hello world";

    args.iter().for_each(|arg| {
        let arg: Vec<&str> = arg.split('=').collect();

        match arg[0] {
            "--port" => port = &arg[1],
            "--body" => body = &arg[1],
            _ => ()
        }
    });

    (port, body)
}

fn handle_connection(mut stream: TcpStream, body: &str) -> Result<(), std::io::Error> {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );

    stream.write_all(response.as_bytes())?;
    stream.flush()
}

#[test]
fn test_parse_parameters() -> Result<(), std::string::FromUtf8Error> {
    let args = vec![
        String::from("exec"),
        String::from("--port=80"),
        String::from("--body=hello"),
    ];
    let (port, body) = parse_parameters(&args);

    assert_eq!(port, "80");
    assert_eq!(body, "hello");

    // Single parameter
    let args = vec![String::from("exec"), String::from("--port=8081")];
    let (port, body) = parse_parameters(&args);

    assert_eq!(port, "8081");
    assert_eq!(body, "hello world");

    // Invert order
    let args = vec![
        String::from("exec"),
        String::from("--body=first"),
        String::from("--port=8082"),
    ];
    let (port, body) = parse_parameters(&args);

    assert_eq!(port, "8082");
    assert_eq!(body, "first");

    Ok(())
}

#[test]
fn test_parse_parameters_with_defaults() -> Result<(), std::string::FromUtf8Error> {
    let args = vec![String::from("exec"), String::from(""), String::from("")];
    let (port, body) = parse_parameters(&args);

    assert_eq!(port, "8080");
    assert_eq!(body, "hello world");

    Ok(())
}
