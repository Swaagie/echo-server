mod common;

use std::time::Duration;

use common::{
    free_port, h2_tls_request, h2c_request, h2c_try_request, run_to_completion, tls_config_toml,
    ClientAuth, Server, TestCerts, TestConfig,
};
use http_body_util::{BodyExt, Empty, Full, StreamBody};
use hyper::body::{Bytes, Frame};
use hyper::{Method, Request, StatusCode};

fn empty() -> Empty<Bytes> {
    Empty::new()
}

fn full(s: &str) -> Full<Bytes> {
    Full::new(Bytes::from(s.to_string()))
}

fn request<B>(method: Method, port: u16, path: &str, body: B) -> Request<B> {
    Request::builder()
        .method(method)
        .uri(format!("http://127.0.0.1:{port}{path}"))
        .body(body)
        .expect("build request")
}

fn h2c_server() -> Server {
    let port = free_port();
    Server::start_tcp(&["--protocol", "h2c", "--port", &port.to_string()], port)
}

#[tokio::test]
async fn h2c_get_root_echoes_request_headers() {
    let server = h2c_server();
    let req = request(Method::GET, server.port, "/", empty());
    let resp = h2c_request(server.addr(), {
        let mut req = req;
        req.headers_mut()
            .insert("x-echo-me", "hello".parse().expect("header value"));
        req
    })
    .await;

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.header("x-echo-me").as_deref(), Some("hello"));
    assert!(
        resp.body.contains("x-echo-me: hello"),
        "body did not echo the header: {:?}",
        resp.body
    );
    resp.assert_content_length_matches_body();
    server.assert_no_panic();
}

#[tokio::test]
async fn h2c_post_root_echoes_headers_and_body() {
    let server = h2c_server();
    let resp = h2c_request(
        server.addr(),
        request(Method::POST, server.port, "/", full("payload-body")),
    )
    .await;

    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.body.ends_with("\n\npayload-body"),
        "body did not append the request body: {:?}",
        resp.body
    );
    resp.assert_content_length_matches_body();
    server.assert_no_panic();
}

#[tokio::test]
async fn h2c_put_and_patch_echo_body() {
    let server = h2c_server();
    for method in [Method::PUT, Method::PATCH] {
        let resp = h2c_request(
            server.addr(),
            request(method.clone(), server.port, "/", full("mutating")),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK, "method {method}");
        assert!(
            resp.body.ends_with("\n\nmutating"),
            "method {method} did not echo body: {:?}",
            resp.body
        );
        resp.assert_content_length_matches_body();
    }
    server.assert_no_panic();
}

#[tokio::test]
async fn h2c_options_root_returns_empty_body() {
    let server = h2c_server();
    let resp = h2c_request(
        server.addr(),
        request(Method::OPTIONS, server.port, "/", empty()),
    )
    .await;

    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.body.is_empty(),
        "OPTIONS returned a body: {:?}",
        resp.body
    );
    resp.assert_content_length_matches_body();
    server.assert_no_panic();
}

#[tokio::test]
async fn h2c_not_found_does_not_advertise_request_content_length() {
    let server = h2c_server();
    let resp = h2c_try_request(
        server.addr(),
        request(Method::POST, server.port, "/nope", full("hello")),
    )
    .await
    .expect("404 response was malformed (stream reset): it declares the request's content-length but sends no body");

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert!(resp.body.is_empty(), "404 returned a body: {:?}", resp.body);
    resp.assert_content_length_matches_body();
    server.assert_no_panic();
}

#[tokio::test]
async fn h2c_options_does_not_advertise_request_content_length() {
    let server = h2c_server();
    let resp = h2c_try_request(
        server.addr(),
        request(Method::OPTIONS, server.port, "/", full("body-on-options")),
    )
    .await
    .expect("OPTIONS response was malformed (stream reset): it declares the request's content-length but sends no body");

    assert_eq!(resp.status(), StatusCode::OK);
    resp.assert_content_length_matches_body();
    server.assert_no_panic();
}

#[tokio::test]
async fn h2c_head_root_mirrors_get() {
    let server = h2c_server();
    let get = h2c_request(
        server.addr(),
        request(Method::GET, server.port, "/", empty()),
    )
    .await;
    let head = h2c_request(
        server.addr(),
        request(Method::HEAD, server.port, "/", empty()),
    )
    .await;

    assert_eq!(head.status(), StatusCode::OK, "HEAD / should mirror GET /");
    assert!(
        head.body.is_empty(),
        "HEAD returned a body: {:?}",
        head.body
    );
    assert_eq!(
        head.header("content-length"),
        get.header("content-length"),
        "HEAD must advertise the length GET would have sent"
    );
    server.assert_no_panic();
}

#[tokio::test]
async fn h2c_does_not_echo_hop_by_hop_headers() {
    let server = h2c_server();
    let mut req = request(Method::GET, server.port, "/", empty());
    req.headers_mut()
        .insert("te", "trailers".parse().expect("header value"));
    let resp = h2c_request(server.addr(), req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    resp.assert_no_hop_by_hop_headers();
    server.assert_no_panic();
}

#[tokio::test]
async fn h2c_truncated_request_body_does_not_panic_the_server() {
    let port = free_port();
    let server = Server::start_tcp(&["--protocol", "h2c", "--port", &port.to_string()], port);

    let stream = tokio::net::TcpStream::connect(server.addr())
        .await
        .expect("connect");
    let (mut sender, conn) = hyper::client::conn::http2::handshake(
        hyper_util::rt::TokioExecutor::new(),
        hyper_util::rt::TokioIo::new(stream),
    )
    .await
    .expect("handshake");
    let conn_task = tokio::spawn(async move {
        let _ = conn.await;
    });

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Frame<Bytes>, std::io::Error>>(4);
    tx.send(Ok(Frame::data(Bytes::from_static(b"partial"))))
        .await
        .expect("queue first chunk");
    let body = StreamBody::new(tokio_stream::wrappers::ReceiverStream::new(rx));

    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("http://127.0.0.1:{port}/"))
        .header("content-length", "1000")
        .body(body)
        .expect("build truncated request");

    let pending = tokio::spawn(async move { sender.send_request(req).await.map(|_| ()) });
    tokio::time::sleep(Duration::from_millis(150)).await;
    conn_task.abort();
    drop(tx);
    let _ = pending.await;

    tokio::time::sleep(Duration::from_millis(400)).await;
    server.assert_no_panic();

    let resp = h2c_request(server.addr(), request(Method::GET, port, "/", empty())).await;
    assert_eq!(resp.status(), StatusCode::OK);
    server.assert_no_panic();
}

fn tls_server(certs: &TestCerts, protocol: &str, mtls: bool) -> (Server, TestConfig, u16) {
    let port = free_port();
    let config = TestConfig::write(&tls_config_toml(certs, port, protocol, mtls));
    let server = Server::start_tcp(&["--config", &config.arg()], port);
    (server, config, port)
}

#[tokio::test]
async fn h2_over_tls_round_trip() {
    let certs = TestCerts::generate();
    let (server, _config, port) = tls_server(&certs, "h2", false);

    let resp = h2_tls_request(
        server.addr(),
        &certs.ca_cert,
        ClientAuth::None,
        request(Method::GET, port, "/", empty()),
    )
    .await
    .expect("h2 over TLS request should succeed");

    assert_eq!(resp.status(), StatusCode::OK);
    resp.assert_content_length_matches_body();
    server.assert_no_panic();
}

#[tokio::test]
async fn mtls_accepts_client_cert_signed_by_configured_ca() {
    let certs = TestCerts::generate();
    let (server, _config, port) = tls_server(&certs, "h2", true);

    let resp = h2_tls_request(
        server.addr(),
        &certs.ca_cert,
        ClientAuth::Cert {
            cert: &certs.client_cert,
            key: &certs.client_key,
        },
        request(Method::GET, port, "/", empty()),
    )
    .await
    .expect("mTLS request with a valid client cert should succeed");

    assert_eq!(resp.status(), StatusCode::OK);
    server.assert_no_panic();
}

#[tokio::test]
async fn mtls_rejects_connection_without_client_cert() {
    let certs = TestCerts::generate();
    let (server, _config, port) = tls_server(&certs, "h2", true);

    let result = h2_tls_request(
        server.addr(),
        &certs.ca_cert,
        ClientAuth::None,
        request(Method::GET, port, "/", empty()),
    )
    .await;

    assert!(
        result.is_err(),
        "server accepted an anonymous client while mTLS was required"
    );
    server.assert_no_panic();
}

#[tokio::test]
async fn mtls_rejects_client_cert_from_untrusted_ca() {
    let certs = TestCerts::generate();
    let (server, _config, port) = tls_server(&certs, "h2", true);

    let result = h2_tls_request(
        server.addr(),
        &certs.ca_cert,
        ClientAuth::Cert {
            cert: &certs.foreign_client_cert,
            key: &certs.foreign_client_key,
        },
        request(Method::GET, port, "/", empty()),
    )
    .await;

    assert!(
        result.is_err(),
        "server accepted a client cert signed by an untrusted CA"
    );
    server.assert_no_panic();
}

#[cfg(feature = "http3")]
mod http3 {
    use super::*;
    use common::h3_request;

    fn h3_server(certs: &TestCerts, mtls: bool) -> (Server, TestConfig, u16) {
        let port = free_port();
        let config = TestConfig::write(&tls_config_toml(certs, port, "h3", mtls));
        let mut server = Server::spawn(&["--config", &config.arg()], port);
        server.wait_for_log("Listening for HTTP/3", Duration::from_secs(10));
        (server, config, port)
    }

    #[tokio::test]
    async fn h3_get_round_trip() {
        let certs = TestCerts::generate();
        let (server, _config, _port) = h3_server(&certs, false);

        let resp = h3_request(
            server.addr(),
            &certs.ca_cert,
            ClientAuth::None,
            Method::GET,
            "/",
            None,
        )
        .await
        .expect("h3 GET should succeed");

        assert_eq!(resp.status(), StatusCode::OK);
        resp.assert_content_length_matches_body();
        server.assert_no_panic();
    }

    #[tokio::test]
    async fn h3_post_round_trip_echoes_body() {
        let certs = TestCerts::generate();
        let (server, _config, _port) = h3_server(&certs, false);

        let resp = h3_request(
            server.addr(),
            &certs.ca_cert,
            ClientAuth::None,
            Method::POST,
            "/",
            Some(Bytes::from_static(b"quic-payload")),
        )
        .await
        .expect("h3 POST should succeed");

        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            resp.body.ends_with("\n\nquic-payload"),
            "h3 body was not echoed: {:?}",
            resp.body
        );
        resp.assert_content_length_matches_body();
        server.assert_no_panic();
    }

    #[tokio::test]
    async fn h3_mtls_rejects_connection_without_client_cert() {
        let certs = TestCerts::generate();
        let (server, _config, _port) = h3_server(&certs, true);

        let result = h3_request(
            server.addr(),
            &certs.ca_cert,
            ClientAuth::None,
            Method::GET,
            "/",
            None,
        )
        .await;

        assert!(
            result.is_err(),
            "h3 server accepted an anonymous client while mTLS was required"
        );
        server.assert_no_panic();
    }
}

#[tokio::test]
async fn auto_without_tls_serves_h2c() {
    let port = free_port();
    let config = TestConfig::write(&format!("port = {port}\nprotocol = \"auto\"\n"));
    let server = Server::start_tcp(&["--config", &config.arg()], port);

    let resp = h2c_request(server.addr(), request(Method::GET, port, "/", empty())).await;
    assert_eq!(resp.status(), StatusCode::OK);
    server.assert_no_panic();
}

#[tokio::test]
async fn auto_with_tls_serves_h2() {
    let certs = TestCerts::generate();
    let (server, _config, port) = tls_server(&certs, "auto", false);

    let resp = h2_tls_request(
        server.addr(),
        &certs.ca_cert,
        ClientAuth::None,
        request(Method::GET, port, "/", empty()),
    )
    .await
    .expect("auto mode should serve h2 over TLS");

    assert_eq!(resp.status(), StatusCode::OK);
    server.assert_no_panic();
}

#[cfg(feature = "http3")]
#[tokio::test]
async fn auto_with_tls_advertises_h3_via_alt_svc() {
    let certs = TestCerts::generate();
    let (server, _config, port) = tls_server(&certs, "auto", false);

    let resp = h2_tls_request(
        server.addr(),
        &certs.ca_cert,
        ClientAuth::None,
        request(Method::GET, port, "/", empty()),
    )
    .await
    .expect("auto mode should serve h2 over TLS");

    let alt_svc = resp
        .header("alt-svc")
        .expect("auto mode must advertise h3 via alt-svc");
    assert!(
        alt_svc.contains("h3=") && alt_svc.contains(&port.to_string()),
        "alt-svc did not point at the h3 listener: {alt_svc:?}"
    );
    server.assert_no_panic();
}

#[cfg(feature = "http3")]
#[tokio::test]
async fn auto_exits_nonzero_when_the_udp_listener_cannot_bind() {
    let certs = TestCerts::generate();
    let port = free_port();

    let _blocker = std::net::UdpSocket::bind(("127.0.0.1", port)).expect("occupy udp port");

    let config = TestConfig::write(&tls_config_toml(&certs, port, "auto", false));
    let mut server = Server::spawn(&["--config", &config.arg()], port);

    let status = server
        .wait_for_exit(Duration::from_secs(5))
        .expect("server should exit when a listener cannot bind");
    assert!(
        !status.success(),
        "server exited successfully despite a listener failing to bind"
    );
    let stderr = server.stderr();
    assert!(
        !stderr.contains("panicked"),
        "server panicked instead of reporting the bind failure; stderr was:\n{stderr}"
    );
    assert!(
        stderr.to_lowercase().contains("address")
            || stderr.to_lowercase().contains("bind")
            || stderr.to_lowercase().contains("in use"),
        "server exited without explaining why; stderr was:\n{stderr}"
    );
}

#[test]
fn missing_certificate_exits_nonzero_with_a_message_on_stderr() {
    let config = TestConfig::write(
        "port = 1\n\n[tls]\nserver_cert = \"/nonexistent/server.pem\"\nserver_key = \"/nonexistent/server.key\"\n",
    );
    let (status, stderr) = run_to_completion(&["--config", &config.arg()]);

    assert!(!status.success(), "expected a non-zero exit");
    assert!(
        stderr.contains("certificate"),
        "startup failure was silent at the default log level; stderr was: {stderr:?}"
    );
}

#[test]
fn unknown_protocol_exits_nonzero_with_a_message_on_stderr() {
    let (status, stderr) =
        run_to_completion(&["--protocol", "h9", "--config", "/nonexistent.toml"]);

    assert!(!status.success(), "expected a non-zero exit");
    assert!(
        stderr.contains("h9") || stderr.to_lowercase().contains("protocol"),
        "unknown protocol failure was silent; stderr was: {stderr:?}"
    );
}

#[test]
fn h2_without_tls_exits_nonzero_with_a_message_on_stderr() {
    let (status, stderr) =
        run_to_completion(&["--protocol", "h2", "--config", "/nonexistent.toml"]);

    assert!(!status.success(), "expected a non-zero exit");
    assert!(
        stderr.to_lowercase().contains("tls"),
        "missing-TLS failure was silent; stderr was: {stderr:?}"
    );
}

#[test]
fn h2c_with_tls_exits_nonzero_with_a_message_on_stderr() {
    let certs = TestCerts::generate();
    let config = TestConfig::write(&tls_config_toml(&certs, 1, "h2c", false));
    let (status, stderr) = run_to_completion(&["--config", &config.arg()]);

    assert!(!status.success(), "expected a non-zero exit");
    assert!(
        stderr.to_lowercase().contains("tls"),
        "h2c-with-TLS failure was silent; stderr was: {stderr:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn sigint_drains_a_request_that_is_still_in_flight() {
    let port = free_port();
    let mut server = Server::start_tcp(&["--protocol", "h2c", "--port", &port.to_string()], port);

    let stream = tokio::net::TcpStream::connect(server.addr())
        .await
        .expect("connect");
    let (mut sender, conn) = hyper::client::conn::http2::handshake(
        hyper_util::rt::TokioExecutor::new(),
        hyper_util::rt::TokioIo::new(stream),
    )
    .await
    .expect("handshake");
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Frame<Bytes>, std::io::Error>>(4);
    let body = StreamBody::new(tokio_stream::wrappers::ReceiverStream::new(rx));
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("http://127.0.0.1:{port}/"))
        .body(body)
        .expect("build streaming request");

    tx.send(Ok(Frame::data(Bytes::from_static(b"first-half "))))
        .await
        .expect("send first chunk");
    let pending = tokio::spawn(async move { sender.send_request(req).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    server.send_sigint();
    tokio::time::sleep(Duration::from_millis(200)).await;

    tx.send(Ok(Frame::data(Bytes::from_static(b"second-half"))))
        .await
        .expect("send second chunk");
    drop(tx);

    let resp = pending
        .await
        .expect("join request task")
        .expect("in-flight request was dropped by shutdown");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let body = String::from_utf8_lossy(&body);
    assert!(
        body.ends_with("\n\nfirst-half second-half"),
        "drained response lost part of the body: {body:?}"
    );

    let status = server
        .wait_for_exit(Duration::from_secs(5))
        .expect("server should exit after draining");
    assert!(status.success(), "expected a clean exit, got {status}");
    server.assert_no_panic();
}

#[test]
fn empty_certificate_path_is_rejected_with_a_clear_message() {
    let config = TestConfig::write(
        "port = 1\n\n[tls]\nserver_cert = \"\"\nserver_key = \"\"\nca_cert = \"\"\nrequire_client_certs = true\n",
    );
    let (status, stderr) = run_to_completion(&["--config", &config.arg()]);

    assert!(!status.success(), "expected a non-zero exit");
    assert!(
        stderr.contains("empty"),
        "empty cert path was not reported as such; stderr was: {stderr:?}"
    );
}

#[test]
fn unparseable_certificate_is_rejected_at_startup() {
    let placeholder = TestConfig::write("placeholder");
    let dir = placeholder
        .path
        .parent()
        .expect("config parent")
        .to_path_buf();
    let junk_cert = dir.join("junk.pem");
    let junk_key = dir.join("junk.key");
    std::fs::write(&junk_cert, "not a certificate").expect("write junk cert");
    std::fs::write(&junk_key, "not a key").expect("write junk key");

    let config = TestConfig::write(&format!(
        "port = 1\n\n[tls]\nserver_cert = \"{}\"\nserver_key = \"{}\"\n",
        junk_cert.display(),
        junk_key.display()
    ));
    let (status, stderr) = run_to_completion(&["--config", &config.arg()]);

    assert!(!status.success(), "expected a non-zero exit");
    assert!(
        stderr.to_lowercase().contains("certificate") || stderr.to_lowercase().contains("key"),
        "unparseable certificate was not reported at startup; stderr was: {stderr:?}"
    );
}
