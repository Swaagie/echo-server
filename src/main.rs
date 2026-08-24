mod config;
mod handler;
mod http2;
mod tls;

#[cfg(feature = "http3")]
mod http3;

use clap::Parser;
use config::Cli;
use config::{merge_config, AppConfig, Protocol};
use log::{error, info};
use std::net::SocketAddr;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    if let Err(e) = run().await {
        error!("{}", e);
        std::process::exit(1);
    }
}

async fn run() -> Result<(), BoxError> {
    if rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .is_err()
    {
        return Err("Failed to install the aws-lc-rs crypto provider".into());
    }

    let cli = Cli::parse();
    let config = merge_config(cli).map_err(|e| format!("Failed to load configuration: {e}"))?;
    let address = SocketAddr::from(([0, 0, 0, 0], config.port));

    match config.protocol {
        Protocol::H2c => serve_h2c(&config, address).await,
        Protocol::H2 => serve_h2(&config, address).await,
        Protocol::H3 => serve_h3(&config, address).await,
        Protocol::Auto => serve_auto(&config, address).await,
    }
}

async fn shutdown_signal() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        error!("Failed to listen for shutdown signal: {}", e);
        std::future::pending::<()>().await;
    }
}

fn require_tls(config: &AppConfig, protocol: &str) -> Result<config::TlsConfig, BoxError> {
    let tls = config
        .tls
        .clone()
        .ok_or_else(|| format!("{protocol} protocol requires TLS configuration"))?;
    if tls.require_client_certs {
        info!("mTLS enabled: client certificates required");
    }
    Ok(tls)
}

async fn serve_h2c(config: &AppConfig, address: SocketAddr) -> Result<(), BoxError> {
    if config.tls.is_some() {
        return Err("h2c protocol does not support TLS. Use h2 or h3 for TLS".into());
    }
    http2::serve_h2c(address, shutdown_signal())
        .await
        .map_err(|e| format!("HTTP/2 cleartext server error: {e}").into())
}

async fn serve_h2(config: &AppConfig, address: SocketAddr) -> Result<(), BoxError> {
    let tls = require_tls(config, "h2")?;
    http2::serve_h2(&address, &tls, None, shutdown_signal())
        .await
        .map_err(|e| format!("HTTP/2 over TLS server error: {e}").into())
}

#[cfg(feature = "http3")]
async fn serve_h3(config: &AppConfig, address: SocketAddr) -> Result<(), BoxError> {
    let tls = require_tls(config, "h3")?;
    http3::serve_h3(&address, &tls, shutdown_signal())
        .await
        .map_err(|e| format!("HTTP/3 server error: {e}").into())
}

#[cfg(not(feature = "http3"))]
async fn serve_h3(_config: &AppConfig, _address: SocketAddr) -> Result<(), BoxError> {
    Err("HTTP/3 support is not compiled in. Build with --features http3".into())
}

#[cfg(feature = "http3")]
async fn serve_auto(config: &AppConfig, address: SocketAddr) -> Result<(), BoxError> {
    let Some(tls) = config.tls.clone() else {
        return serve_h2c(config, address).await;
    };
    if tls.require_client_certs {
        info!("mTLS enabled: client certificates required");
    }

    let alt_svc = format!("h3=\":{}\"; ma=3600", address.port());

    let h2 = async {
        http2::serve_h2(&address, &tls, Some(alt_svc), shutdown_signal())
            .await
            .map_err(|e| BoxError::from(format!("HTTP/2 server error: {e}")))
    };
    let h3 = async {
        http3::serve_h3(&address, &tls, shutdown_signal())
            .await
            .map_err(|e| BoxError::from(format!("HTTP/3 server error: {e}")))
    };

    tokio::try_join!(h2, h3).map(|((), ())| ())
}

#[cfg(not(feature = "http3"))]
async fn serve_auto(config: &AppConfig, address: SocketAddr) -> Result<(), BoxError> {
    match config.tls {
        Some(_) => serve_h2(config, address).await,
        None => serve_h2c(config, address).await,
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use handler::{format_headers, handle_request};
    use http_body_util::{Empty, Full};
    use hyper::body::Bytes;
    use hyper::header::HeaderValue;
    use hyper::{Method, Request};
    use std::fs;
    use std::path::PathBuf;

    fn create_temp_file(contents: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);

        let temp_dir = std::env::temp_dir();
        let file_name = format!(
            "echo-server-test-{}-{}.toml",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
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
        req.headers_mut()
            .insert("x-test-header", HeaderValue::from_static("test-value"));
        *req.method_mut() = Method::GET;
        *req.uri_mut() = hyper::Uri::from_static("http://localhost/");

        let resp = tokio_test::block_on(handle_request(req)).unwrap();

        assert_eq!(resp.status(), hyper::StatusCode::OK);
        assert!(resp.headers().contains_key("content-type"));
        assert!(resp.headers().contains_key("x-test-header"));

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
        req.headers_mut()
            .insert("x-custom-header", HeaderValue::from_static("value"));
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
        req.headers_mut()
            .insert("x-echo-test", HeaderValue::from_static("should-be-echoed"));
        req.headers_mut()
            .insert("another-header", HeaderValue::from_static("another-value"));
        *req.method_mut() = Method::GET;
        *req.uri_mut() = hyper::Uri::from_static("http://localhost/");

        let resp = tokio_test::block_on(handle_request(req)).unwrap();

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

        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_load_config_file_invalid_toml() {
        let temp_file = create_temp_file("invalid toml content [");
        let file_path = temp_file.to_str().unwrap();

        let result = config::load_config_file(file_path);
        assert!(result.is_err());

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
        assert_eq!(config.port, 3000);

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
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn test_merge_config_tls_from_file() {
        let temp_dir = std::env::temp_dir().join(format!(
            "echo-server-test-certs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();
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

        let _ = fs::remove_file(&temp_file);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
