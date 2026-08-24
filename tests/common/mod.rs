//! Shared harness for end-to-end tests.
//!
//! Every test in this suite talks to a real `echo-server` process over a real
//! socket. Nothing here calls into the library directly: the point is to catch
//! defects that only appear once a listener is bound and a handshake completes.

#![allow(dead_code)]

use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use http_body_util::BodyExt;
use hyper::body::Bytes;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};

/// Install `aws-lc-rs` as the process-level rustls provider for the *test*
/// process, matching the provider the server is expected to pin (chosen for its
/// post-quantum key exchange support).
///
/// This only makes the client side deterministic; the server must install its
/// own, so a handshake failure here points at the server.
pub fn install_crypto_provider() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

/// Grab a port the OS says is free.
///
/// There is an inherent race between releasing the probe socket and the server
/// binding it, so each test gets a fresh port and retries are cheap.
pub fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind probe socket");
    let port = listener.local_addr().expect("probe local_addr").port();
    drop(listener);
    port
}

// ---------------------------------------------------------------------------
// Certificate fixtures
// ---------------------------------------------------------------------------

/// A throwaway PKI: one CA, a server leaf, a client leaf signed by that CA, and
/// a second CA with its own client leaf to exercise rejection paths.
pub struct TestCerts {
    dir: PathBuf,
    pub ca_cert: PathBuf,
    pub server_cert: PathBuf,
    pub server_key: PathBuf,
    pub client_cert: PathBuf,
    pub client_key: PathBuf,
    pub foreign_client_cert: PathBuf,
    pub foreign_client_key: PathBuf,
}

impl TestCerts {
    pub fn generate() -> Self {
        use rcgen::{
            BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer,
            KeyPair, KeyUsagePurpose,
        };

        let dir = scratch_dir("certs");
        std::fs::create_dir_all(&dir).expect("create cert dir");

        fn ca(name: &str) -> (CertificateParams, KeyPair, String) {
            let mut params = CertificateParams::new(Vec::new()).expect("ca params");
            params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
            params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
            params.distinguished_name.push(DnType::CommonName, name);
            let key = KeyPair::generate().expect("ca key");
            let pem = params.self_signed(&key).expect("self-sign ca").pem();
            (params, key, pem)
        }

        fn leaf(
            issuer: &Issuer<'_, &KeyPair>,
            sans: Vec<String>,
            common_name: &str,
            usage: ExtendedKeyUsagePurpose,
        ) -> (String, String) {
            let mut params = CertificateParams::new(sans).expect("leaf params");
            params.is_ca = IsCa::NoCa;
            params.extended_key_usages = vec![usage];
            params
                .distinguished_name
                .push(DnType::CommonName, common_name);
            let key = KeyPair::generate().expect("leaf key");
            let cert = params.signed_by(&key, issuer).expect("sign leaf");
            (cert.pem(), key.serialize_pem())
        }

        let (ca_params, ca_key, ca_pem) = ca("echo-server-e2e-ca");
        let issuer = Issuer::from_params(&ca_params, &ca_key);

        let (server_pem, server_key_pem) = leaf(
            &issuer,
            vec!["localhost".to_string(), "127.0.0.1".to_string()],
            "echo-server-e2e",
            ExtendedKeyUsagePurpose::ServerAuth,
        );
        let (client_pem, client_key_pem) = leaf(
            &issuer,
            vec!["client.e2e".to_string()],
            "echo-server-e2e-client",
            ExtendedKeyUsagePurpose::ClientAuth,
        );

        let (foreign_params, foreign_key, _) = ca("echo-server-e2e-foreign-ca");
        let foreign_issuer = Issuer::from_params(&foreign_params, &foreign_key);
        let (foreign_pem, foreign_key_pem) = leaf(
            &foreign_issuer,
            vec!["intruder.e2e".to_string()],
            "echo-server-e2e-intruder",
            ExtendedKeyUsagePurpose::ClientAuth,
        );

        let write = |name: &str, contents: &str| -> PathBuf {
            let path = dir.join(name);
            std::fs::write(&path, contents).expect("write pem");
            path
        };

        TestCerts {
            ca_cert: write("ca.pem", &ca_pem),
            server_cert: write("server.pem", &server_pem),
            server_key: write("server.key", &server_key_pem),
            client_cert: write("client.pem", &client_pem),
            client_key: write("client.key", &client_key_pem),
            foreign_client_cert: write("intruder.pem", &foreign_pem),
            foreign_client_key: write("intruder.key", &foreign_key_pem),
            dir,
        }
    }
}

impl Drop for TestCerts {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn scratch_dir(kind: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("echo-server-e2e-{kind}-{nanos}-{n}"))
}

/// Write a config file into a self-cleaning temp directory.
pub struct TestConfig {
    dir: PathBuf,
    pub path: PathBuf,
}

impl TestConfig {
    pub fn write(contents: &str) -> Self {
        let dir = scratch_dir("config");
        std::fs::create_dir_all(&dir).expect("create config dir");
        let path = dir.join("config.toml");
        std::fs::write(&path, contents).expect("write config");
        TestConfig { dir, path }
    }

    pub fn arg(&self) -> String {
        self.path.to_string_lossy().to_string()
    }
}

impl Drop for TestConfig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Render a TLS config file pointing at the generated fixtures.
pub fn tls_config_toml(certs: &TestCerts, port: u16, protocol: &str, mtls: bool) -> String {
    format!(
        "port = {port}\nprotocol = \"{protocol}\"\n\n[tls]\nserver_cert = \"{}\"\nserver_key = \"{}\"\nca_cert = \"{}\"\nrequire_client_certs = {mtls}\n",
        display(&certs.server_cert),
        display(&certs.server_key),
        display(&certs.ca_cert),
    )
}

fn display(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

// ---------------------------------------------------------------------------
// Server process
// ---------------------------------------------------------------------------

/// A running `echo-server` child process, killed when the handle is dropped.
pub struct Server {
    child: Option<Child>,
    pub port: u16,
    stderr: Arc<Mutex<String>>,
}

impl Server {
    /// Start the server and wait until its TCP listener accepts connections.
    pub fn start_tcp(args: &[&str], port: u16) -> Server {
        let mut server = Server::spawn(args, port);
        server.wait_for_tcp();
        server
    }

    /// Start the server without waiting for a TCP listener (UDP-only modes).
    pub fn spawn(args: &[&str], port: u16) -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_echo-server"))
            .args(args)
            .env("RUST_LOG", "debug")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn echo-server");

        let stderr = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&stderr);
        let mut pipe = child.stderr.take().expect("stderr pipe");
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match pipe.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut guard = sink.lock().expect("stderr lock");
                        guard.push_str(&String::from_utf8_lossy(&buf[..n]));
                    }
                }
            }
        });

        Server {
            child: Some(child),
            port,
            stderr,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.port))
    }

    /// Everything the server has written to stderr so far.
    pub fn stderr(&self) -> String {
        self.stderr.lock().expect("stderr lock").clone()
    }

    /// Fail the test if the server logged a panic.
    pub fn assert_no_panic(&self) {
        let log = self.stderr();
        assert!(
            !log.contains("panicked"),
            "server panicked; stderr was:\n{log}"
        );
    }

    fn wait_for_tcp(&mut self) {
        let addr = self.addr();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let exited = self
                .child
                .as_mut()
                .expect("child already reaped")
                .try_wait()
                .expect("try_wait");
            if let Some(status) = exited {
                panic!(
                    "server exited early with {status}; stderr was:\n{}",
                    self.stderr()
                );
            }
            if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!(
            "server did not accept TCP on {addr} within 10s; stderr was:\n{}",
            self.stderr()
        );
    }

    /// Wait up to `timeout` for the process to exit on its own.
    pub fn wait_for_exit(&mut self, timeout: Duration) -> Option<std::process::ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            let exited = self
                .child
                .as_mut()
                .expect("child already reaped")
                .try_wait()
                .expect("try_wait");
            if exited.is_some() {
                return exited;
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Send SIGINT without waiting. Safe to call from async context: it does
    /// not block the runtime thread.
    #[cfg(unix)]
    pub fn send_sigint(&mut self) {
        let pid = self.child.as_mut().expect("child already reaped").id();
        let killed = Command::new("kill")
            .arg("-INT")
            .arg(pid.to_string())
            .status()
            .expect("send SIGINT");
        assert!(killed.success(), "failed to signal pid {pid}");
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Run the server to completion and capture its exit status and stderr.
pub fn run_to_completion(args: &[&str]) -> (std::process::ExitStatus, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_echo-server"))
        .args(args)
        .env_remove("RUST_LOG")
        .output()
        .expect("run echo-server");
    (
        output.status,
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

// ---------------------------------------------------------------------------
// Clients
// ---------------------------------------------------------------------------

/// A response with its body already collected.
pub struct Captured {
    pub parts: hyper::http::response::Parts,
    pub body: String,
}

impl Captured {
    pub fn status(&self) -> hyper::StatusCode {
        self.parts.status
    }

    pub fn header(&self, name: &str) -> Option<String> {
        self.parts
            .headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    }

    /// A response must never advertise a length it did not send.
    pub fn assert_content_length_matches_body(&self) {
        if let Some(declared) = self.header("content-length") {
            let declared: usize = declared.parse().expect("numeric content-length");
            assert_eq!(
                declared,
                self.body.len(),
                "content-length {declared} does not match body of {} bytes: {:?}",
                self.body.len(),
                self.body
            );
        }
    }

    /// Connection-level headers are per-hop and must not be echoed back.
    pub fn assert_no_hop_by_hop_headers(&self) {
        for name in [
            "connection",
            "transfer-encoding",
            "keep-alive",
            "proxy-connection",
            "te",
            "trailer",
            "upgrade",
        ] {
            assert!(
                !self.parts.headers.contains_key(name),
                "response echoed hop-by-hop header {name:?}: {:?}",
                self.parts.headers
            );
        }
    }
}

async fn capture<B>(resp: Response<B>) -> Captured
where
    B: hyper::body::Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Debug,
{
    let (parts, body) = resp.into_parts();
    let bytes = body.collect().await.expect("collect body").to_bytes();
    Captured {
        parts,
        body: String::from_utf8_lossy(&bytes).to_string(),
    }
}

/// Send one request over h2c (prior knowledge, no TLS).
pub async fn h2c_request<B>(addr: SocketAddr, req: Request<B>) -> Captured
where
    B: hyper::body::Body<Data = Bytes> + Send + Unpin + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let stream = tokio::net::TcpStream::connect(addr)
        .await
        .expect("connect h2c");
    let (mut sender, conn) =
        hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(stream))
            .await
            .expect("h2c handshake");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let resp = sender.send_request(req).await.expect("h2c request");
    capture(resp).await
}

/// Send one request over h2c, returning the transport error if there is one.
pub async fn h2c_try_request<B>(addr: SocketAddr, req: Request<B>) -> Result<Captured, hyper::Error>
where
    B: hyper::body::Body<Data = Bytes> + Send + Unpin + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let stream = tokio::net::TcpStream::connect(addr)
        .await
        .expect("connect h2c");
    let (mut sender, conn) =
        hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(stream)).await?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let resp = sender.send_request(req).await?;
    Ok(capture(resp).await)
}

/// Client identity for a TLS connection.
pub enum ClientAuth<'a> {
    None,
    Cert { cert: &'a Path, key: &'a Path },
}

fn load_pem_certs(path: &Path) -> Vec<rustls_pki_types::CertificateDer<'static>> {
    let contents = std::fs::read(path).expect("read cert pem");
    rustls_pki_types::pem::SliceIter::<rustls_pki_types::CertificateDer>::new(&contents)
        .map(|c| c.expect("parse cert pem"))
        .collect()
}

fn load_pem_key(path: &Path) -> rustls_pki_types::PrivateKeyDer<'static> {
    let contents = std::fs::read(path).expect("read key pem");
    rustls_pki_types::pem::SliceIter::<rustls_pki_types::PrivatePkcs8KeyDer>::new(&contents)
        .next()
        .expect("pkcs8 key in pem")
        .map(rustls_pki_types::PrivateKeyDer::Pkcs8)
        .expect("parse key pem")
}

fn client_tls_config(ca_cert: &Path, auth: ClientAuth<'_>, alpn: &[&[u8]]) -> rustls::ClientConfig {
    install_crypto_provider();

    let mut roots = rustls::RootCertStore::empty();
    for cert in load_pem_certs(ca_cert) {
        roots.add(cert).expect("add ca to root store");
    }

    let builder = rustls::ClientConfig::builder().with_root_certificates(roots);
    let mut config = match auth {
        ClientAuth::None => builder.with_no_client_auth(),
        ClientAuth::Cert { cert, key } => builder
            .with_client_auth_cert(load_pem_certs(cert), load_pem_key(key))
            .expect("client auth cert"),
    };
    config.alpn_protocols = alpn.iter().map(|p| p.to_vec()).collect();
    config
}

/// Complete a TLS handshake and send one h2 request.
pub async fn h2_tls_request<B>(
    addr: SocketAddr,
    ca_cert: &Path,
    auth: ClientAuth<'_>,
    req: Request<B>,
) -> Result<Captured, Box<dyn std::error::Error + Send + Sync>>
where
    B: hyper::body::Body<Data = Bytes> + Send + Unpin + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let config = client_tls_config(ca_cert, auth, &[b"h2"]);
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let server_name = rustls_pki_types::ServerName::try_from("localhost")?;

    let stream = tokio::net::TcpStream::connect(addr).await?;
    let tls = connector.connect(server_name, stream).await?;
    let (mut sender, conn) =
        hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(tls)).await?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let resp = sender.send_request(req).await?;
    Ok(capture(resp).await)
}

/// Complete a QUIC handshake and send one h3 request.
#[cfg(feature = "http3")]
pub async fn h3_request(
    addr: SocketAddr,
    ca_cert: &Path,
    auth: ClientAuth<'_>,
    method: hyper::Method,
    path: &str,
    body: Option<Bytes>,
) -> Result<Captured, Box<dyn std::error::Error + Send + Sync>> {
    use quinn::crypto::rustls::QuicClientConfig;

    let mut config = client_tls_config(ca_cert, auth, &[b"h3"]);
    config.enable_early_data = true;
    let quic_config = QuicClientConfig::try_from(config)?;
    let client_config = quinn::ClientConfig::new(Arc::new(quic_config));

    let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse()?)?;
    endpoint.set_default_client_config(client_config);

    let conn = endpoint.connect(addr, "localhost")?.await?;
    let quinn_conn = h3_quinn::Connection::new(conn);
    let (mut driver, mut send_request) = h3::client::new(quinn_conn).await?;

    let drive = tokio::spawn(async move { std::future::poll_fn(|cx| driver.poll_close(cx)).await });

    let uri: hyper::Uri = format!("https://localhost{path}").parse()?;
    let req = Request::builder().method(method).uri(uri).body(())?;

    let mut stream = send_request.send_request(req).await?;
    if let Some(body) = body {
        stream.send_data(body).await?;
    }
    stream.finish().await?;

    let resp = stream.recv_response().await?;
    let mut collected = Vec::new();
    while let Some(mut chunk) = stream.recv_data().await? {
        use hyper::body::Buf;
        while chunk.has_remaining() {
            let piece = chunk.chunk().to_vec();
            let n = piece.len();
            collected.extend_from_slice(&piece);
            chunk.advance(n);
        }
    }

    drop(send_request);
    let _ = drive.await;
    endpoint.wait_idle().await;

    let (parts, ()) = resp.into_parts();
    Ok(Captured {
        parts,
        body: String::from_utf8_lossy(&collected).to_string(),
    })
}
