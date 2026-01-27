mod config;
mod handler;
mod tls;
mod http2;

#[cfg(feature = "http3")]
mod http3;

use std::net::SocketAddr;
use config::{merge_config, Protocol};
use config::Cli;
use clap::Parser;
use log::debug;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    let config = match merge_config(cli) {
        Ok(config) => config,
        Err(e) => {
            debug!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    let port = config.port;
    let address = SocketAddr::from(([0, 0, 0, 0], port));

    // Allow server to be killed.
    let shutdown = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to add signal handler")
    };

    // Determine which protocols to start based on configuration
    match config.protocol {
        Protocol::H2c => {
            if config.tls.is_some() {
                debug!("Error: H2c protocol does not support TLS. Use H2 or H3 for TLS.");
                std::process::exit(1);
            }
            if let Err(e) = http2::serve_h2c(address, shutdown).await {
                debug!("HTTP/2 cleartext server error: {}", e);
                std::process::exit(1);
            }
        }
        Protocol::H2 => {
            let tls_config = match config.tls {
                Some(tls) => tls,
                None => {
                    debug!("Error: H2 protocol requires TLS configuration.");
                    std::process::exit(1);
                }
            };
            if tls_config.require_client_certs {
                debug!("mTLS enabled: client certificates required");
            }
            if let Err(e) = http2::serve_h2(&address, &tls_config, shutdown).await {
                debug!("HTTP/2 over TLS server error: {}", e);
                std::process::exit(1);
            }
        }
        Protocol::H3 => {
            #[cfg(not(feature = "http3"))]
            {
                debug!("Error: HTTP/3 support is not compiled in. Build with --features http3");
                std::process::exit(1);
            }
            #[cfg(feature = "http3")]
            {
                let tls_config = match config.tls {
                    Some(tls) => tls,
                    None => {
                        debug!("Error: H3 protocol requires TLS configuration.");
                        std::process::exit(1);
                    }
                };
                if tls_config.require_client_certs {
                    debug!("mTLS enabled: client certificates required");
                }
                if let Err(e) = http3::serve_h3(&address, &tls_config, shutdown).await {
                    debug!("HTTP/3 server error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Protocol::Auto => {
            if let Some(tls_config) = config.tls {
                if tls_config.require_client_certs {
                    debug!("mTLS enabled: client certificates required");
                }
                 let shutdown_h2 = async {
                    tokio::signal::ctrl_c()
                        .await
                        .expect("Failed to add signal handler")
                };
                #[cfg(feature = "http3")]
                let shutdown_h3 = async {
                    tokio::signal::ctrl_c()
                        .await
                        .expect("Failed to add signal handler")
                };

                #[cfg(feature = "http3")]
                {
                    tokio::select! {
                        result = http2::serve_h2(&address, &tls_config, shutdown_h2) => {
                            if let Err(e) = result {
                                debug!("HTTP/2 server error: {}", e);
                            }
                        }
                        result = http3::serve_h3(&address, &tls_config, shutdown_h3) => {
                            if let Err(e) = result {
                                debug!("HTTP/3 server error: {}", e);
                            }
                        }
                    }
                }

                #[cfg(not(feature = "http3"))]
                {
                    // Only start H2 if HTTP/3 is not available
                    if let Err(e) = http2::serve_h2(&address, &tls_config, shutdown_h2).await {
                        debug!("HTTP/2 server error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                // No TLS, start H2c only
                if let Err(e) = http2::serve_h2c(address, shutdown).await {
                    debug!("HTTP/2 cleartext server error: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use http_body_util::{Empty, Full};
    use hyper::header::HeaderValue;
    use hyper::{Method, Request};
    use hyper::body::Bytes;
    use handler::{format_headers, handle_request};

    fn create_temp_file(contents: &str) -> PathBuf {
        let temp_dir = std::env::temp_dir();
        let file_name = format!("echo-server-test-{}.toml", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos());
        let file_path = temp_dir.join(file_name);

        fs::write(&file_path, contents).unwrap();
        file_path
    }

    #[test]
    fn test_format_headers() {
        let mut headers = hyper::HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert("x-custom-header", HeaderValue::from_static("test-value"));

        let formatted = format_headers(&headers);
        assert!(formatted.contains("content-type: application/json"));
        assert!(formatted.contains("x-custom-header: test-value"));
    }

    #[test]
    fn test_format_headers_empty() {
        let headers = hyper::HeaderMap::new();
        let formatted = format_headers(&headers);
        assert_eq!(formatted, "");
    }

    #[test]
    fn test_handle_request_get() {
        let mut req = Request::new(Empty::<Bytes>::new());
        req.headers_mut().insert(
            hyper::header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain"),
        );
        req.headers_mut().insert(
            "x-test-header",
            HeaderValue::from_static("test-value"),
        );
        *req.method_mut() = Method::GET;
        *req.uri_mut() = hyper::Uri::from_static("http://localhost/");

        let resp = tokio_test::block_on(handle_request(req)).unwrap();

        assert_eq!(resp.status(), hyper::StatusCode::OK);
        assert!(resp.headers().contains_key("content-type"));
        assert!(resp.headers().contains_key("x-test-header"));

        // Check that response body contains headers
        let body_str = resp.into_body();
        assert!(body_str.contains("content-type: text/plain"));
        assert!(body_str.contains("x-test-header: test-value"));
    }

    #[test]
    fn test_handle_request_post() {
        let mut req = Request::new(Full::new(Bytes::from("test body content")));
        req.headers_mut().insert(
            hyper::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        *req.method_mut() = Method::POST;
        *req.uri_mut() = hyper::Uri::from_static("http://localhost/");

        let resp = tokio_test::block_on(handle_request(req)).unwrap();

        assert_eq!(resp.status(), hyper::StatusCode::OK);

        let body_str = resp.into_body();
        assert!(body_str.contains("content-type: application/json"));
        assert!(body_str.contains("test body content"));
        assert!(body_str.contains("\n\ntest body content"));
    }

    #[test]
    fn test_handle_request_post_empty_body() {
        let mut req = Request::new(Empty::<Bytes>::new());
        req.headers_mut().insert(
            "x-custom-header",
            HeaderValue::from_static("value"),
        );
        *req.method_mut() = Method::POST;
        *req.uri_mut() = hyper::Uri::from_static("http://localhost/");

        let resp = tokio_test::block_on(handle_request(req)).unwrap();

        assert_eq!(resp.status(), hyper::StatusCode::OK);

        let body_str = resp.into_body();
        assert!(body_str.contains("x-custom-header: value"));
        assert!(!body_str.contains("\n\n"));
    }

    #[test]
    fn test_handle_request_put() {
        let mut req = Request::new(Full::new(Bytes::from("put data")));
        *req.method_mut() = Method::PUT;
        *req.uri_mut() = hyper::Uri::from_static("http://localhost/");

        let resp = tokio_test::block_on(handle_request(req)).unwrap();
        assert_eq!(resp.status(), hyper::StatusCode::OK);
    }

    #[test]
    fn test_handle_request_patch() {
        let mut req = Request::new(Full::new(Bytes::from("patch data")));
        *req.method_mut() = Method::PATCH;
        *req.uri_mut() = hyper::Uri::from_static("http://localhost/");

        let resp = tokio_test::block_on(handle_request(req)).unwrap();
        assert_eq!(resp.status(), hyper::StatusCode::OK);
    }

    #[test]
    fn test_handle_request_options() {
        let mut req = Request::new(Empty::<Bytes>::new());
        *req.method_mut() = Method::OPTIONS;
        *req.uri_mut() = hyper::Uri::from_static("http://localhost/");

        let resp = tokio_test::block_on(handle_request(req)).unwrap();

        assert_eq!(resp.status(), hyper::StatusCode::OK);
        // OPTIONS should not have a body
        let body_str = resp.into_body();
        assert_eq!(body_str.len(), 0);
    }

    #[test]
    fn test_handle_request_not_found() {
        let mut req = Request::new(Empty::<Bytes>::new());
        *req.method_mut() = Method::GET;
        *req.uri_mut() = hyper::Uri::from_static("http://localhost/unknown");

        let resp = tokio_test::block_on(handle_request(req)).unwrap();

        assert_eq!(resp.status(), hyper::StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_handle_request_headers_echoed() {
        let mut req = Request::new(Empty::<Bytes>::new());
        req.headers_mut().insert(
            "x-echo-test",
            HeaderValue::from_static("should-be-echoed"),
        );
        req.headers_mut().insert(
            "another-header",
            HeaderValue::from_static("another-value"),
        );
        *req.method_mut() = Method::GET;
        *req.uri_mut() = hyper::Uri::from_static("http://localhost/");

        let resp = tokio_test::block_on(handle_request(req)).unwrap();

        // Headers should be echoed in response headers
        assert_eq!(
            resp.headers().get("x-echo-test").unwrap(),
            "should-be-echoed"
        );
        assert_eq!(
            resp.headers().get("another-header").unwrap(),
            "another-value"
        );
    }

    #[test]
    fn test_load_config_file_not_exists() {
        let result = config::load_config_file("/nonexistent/path/config.toml").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_load_config_file_valid() {
        let contents = r#"
port = 9000

[tls]
server_cert = "/path/to/cert"
server_key = "/path/to/key"
ca_cert = "/path/to/ca"
"#;
        let temp_file = create_temp_file(contents);
        let file_path = temp_file.to_str().unwrap();

        let result = config::load_config_file(file_path).unwrap();
        assert!(result.is_some());
        let config = result.unwrap();
        assert_eq!(config.port, Some(9000));
        assert!(config.tls.is_some());
        let tls = config.tls.unwrap();
        assert_eq!(tls.server_cert, "/path/to/cert");
        assert_eq!(tls.server_key, "/path/to/key");
        assert_eq!(tls.ca_cert, Some("/path/to/ca".to_string()));

        // Clean up
        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_load_config_file_invalid_toml() {
        let temp_file = create_temp_file("invalid toml content [");
        let file_path = temp_file.to_str().unwrap();

        let result = config::load_config_file(file_path);
        assert!(result.is_err());

        // Clean up
        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_merge_config_cli_port_takes_precedence() {
        let temp_file = create_temp_file("port = 9000");
        let file_path = temp_file.to_str().unwrap().to_string();

        let cli = config::Cli {
            port: Some(3000),
            config: file_path.clone(),
            protocol: None,
        };

        let config = config::merge_config(cli).unwrap();
        assert_eq!(config.port, 3000); // CLI takes precedence

        // Clean up
        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_merge_config_file_port_when_cli_none() {
        let temp_file = create_temp_file("port = 9000");
        let file_path = temp_file.to_str().unwrap().to_string();

        let cli = config::Cli {
            port: None,
            config: file_path.clone(),
            protocol: None,
        };

        let config = config::merge_config(cli).unwrap();
        assert_eq!(config.port, 9000);

        // Clean up
        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_merge_config_default_port() {
        let cli = config::Cli {
            port: None,
            config: "/nonexistent/config.toml".to_string(),
            protocol: None,
        };

        let config = config::merge_config(cli).unwrap();
        assert_eq!(config.port, 8080); // Default port
    }

    #[test]
    fn test_merge_config_tls_from_file() {
        // Create temporary certificate files for validation
        let temp_dir = std::env::temp_dir();
        let server_cert = temp_dir.join("test_server.crt");
        let server_key = temp_dir.join("test_server.key");
        let ca_cert = temp_dir.join("test_ca.crt");

        fs::write(&server_cert, "dummy cert").unwrap();
        fs::write(&server_key, "dummy key").unwrap();
        fs::write(&ca_cert, "dummy ca").unwrap();

        let contents = format!(
            r#"
[tls]
server_cert = "{}"
server_key = "{}"
ca_cert = "{}"
"#,
            server_cert.to_str().unwrap(),
            server_key.to_str().unwrap(),
            ca_cert.to_str().unwrap()
        );
        let temp_file = create_temp_file(&contents);
        let file_path = temp_file.to_str().unwrap().to_string();

        let cli = config::Cli {
            port: None,
            config: file_path.clone(),
            protocol: None,
        };

        let config = config::merge_config(cli).unwrap();
        assert!(config.tls.is_some());
        let tls = config.tls.unwrap();
        assert_eq!(tls.server_cert, server_cert.to_str().unwrap());
        assert_eq!(tls.server_key, server_key.to_str().unwrap());
        assert_eq!(tls.ca_cert, Some(ca_cert.to_str().unwrap().to_string()));

        // Clean up
        let _ = fs::remove_file(&temp_file);
        let _ = fs::remove_file(&server_cert);
        let _ = fs::remove_file(&server_key);
        let _ = fs::remove_file(&ca_cert);
    }
}
