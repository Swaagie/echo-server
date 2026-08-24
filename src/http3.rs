#[cfg(feature = "http3")]
use crate::config::TlsConfig;
#[cfg(feature = "http3")]
use crate::handler::handle_request;
#[cfg(feature = "http3")]
use crate::tls::create_quic_server_config;
#[cfg(feature = "http3")]
use http_body_util::Full;
#[cfg(feature = "http3")]
use hyper::body::Buf;
#[cfg(feature = "http3")]
use hyper::body::Bytes;
#[cfg(feature = "http3")]
use hyper::Request;
#[cfg(feature = "http3")]
use log::{debug, info};
#[cfg(feature = "http3")]
use std::net::SocketAddr;

#[cfg(feature = "http3")]
pub async fn serve_h3(
    address: &SocketAddr,
    tls_config: &TlsConfig,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use h3::server::Connection as H3Connection;
    use h3_quinn::Connection;
    use quinn::Endpoint;
    use tokio::sync::oneshot;

    let server_config = create_quic_server_config(tls_config)?;
    let endpoint = Endpoint::server(server_config, *address)?;
    info!("Listening for HTTP/3 over QUIC (h3) on {}", address);

    let shutdown_signal = Box::pin(shutdown);
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        shutdown_signal.await;
        let _ = shutdown_tx.send(());
    });

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                debug!("Shutting down HTTP/3 server...");
                endpoint.close(0u32.into(), b"shutdown");
                break;
            }
            conn = endpoint.accept() => {
                match conn {
                    Some(conn) => {
                        let quinn_conn = match conn.await {
                            Ok(c) => c,
                            Err(e) => {
                                debug!("QUIC connection error: {}", e);
                                continue;
                            }
                        };

                        tokio::spawn(async move {
                            let h3_conn = Connection::new(quinn_conn);
                            match H3Connection::new(h3_conn).await {
                                Ok(mut server) => {
                                    if let Err(e) = handle_h3_connection(&mut server).await {
                                        debug!("HTTP/3 connection error: {}", e);
                                    }
                                }
                                Err(e) => {
                                    debug!("HTTP/3 handshake error: {}", e);
                                }
                            }
                        });
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(feature = "http3")]
async fn handle_h3_connection(
    conn: &mut h3::server::Connection<h3_quinn::Connection, Bytes>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    loop {
        match conn.accept().await {
            Ok(Some(resolver)) => {
                let (req, stream) = resolver.resolve_request().await?;
                tokio::spawn(async move {
                    if let Err(e) = handle_h3_request(req, stream).await {
                        debug!("Error handling HTTP/3 request: {}", e);
                    }
                });
            }
            Ok(None) => {
                break;
            }
            Err(e) => {
                debug!("Error accepting HTTP/3 request: {}", e);
                break;
            }
        }
    }

    Ok(())
}

#[cfg(feature = "http3")]
async fn handle_h3_request(
    req: Request<()>,
    mut stream: h3::server::RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (parts, _) = req.into_parts();
    let mut hyper_req = Request::new(Full::new(Bytes::new()));
    *hyper_req.method_mut() = parts.method;
    *hyper_req.uri_mut() = parts.uri;
    *hyper_req.headers_mut() = parts.headers;
    *hyper_req.version_mut() = parts.version;

    let mut body_bytes = Vec::new();
    while let Some(chunk) = stream.recv_data().await? {
        body_bytes.extend_from_slice(chunk.chunk());
    }

    if !body_bytes.is_empty() {
        *hyper_req.body_mut() = Full::new(Bytes::from(body_bytes));
    }

    let hyper_resp = handle_request(hyper_req).await?;

    let (parts, body) = hyper_resp.into_parts();
    let body_bytes = Bytes::from(body.into_bytes());

    let mut resp = hyper::Response::builder().status(parts.status).body(())?;
    *resp.headers_mut() = parts.headers;

    stream.send_response(resp).await?;

    if !body_bytes.is_empty() {
        stream.send_data(body_bytes).await?;
    }

    stream.finish().await?;

    Ok(())
}
