use crate::config::TlsConfig;
use crate::handler::handle_request;
use crate::tls::create_tls_config;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use log::debug;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

pub async fn serve_h2c(
    address: SocketAddr,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    debug!("Starting HTTP/2 cleartext server on {}", address);
    let listener = TcpListener::bind(&address).await?;
    let http = hyper::server::conn::http2::Builder::new(TokioExecutor::new());
    let mut shutdown_signal = Box::pin(shutdown);

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        let http = http.clone();
                        let service = service_fn(handle_request);

                        tokio::spawn(async move {
                            let io = TokioIo::new(stream);
                            if let Err(e) = http.serve_connection(io, service).await {
                                debug!("Connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        debug!("Accept error: {}", e);
                    }
                }
            }
            _ = &mut shutdown_signal => {
                debug!("Shutting down HTTP/2 cleartext server...");
                break;
            }
        }
    }

    Ok(())
}

pub async fn serve_h2(
    address: &SocketAddr,
    tls_config: &TlsConfig,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    debug!("Starting HTTP/2 over TLS server on {}", address);

    let tls_server_config = create_tls_config(tls_config)?;

    let tls_acceptor = TlsAcceptor::from(tls_server_config);
    let listener = TcpListener::bind(&address).await?;
    let http = hyper::server::conn::http2::Builder::new(TokioExecutor::new());
    let mut shutdown_signal = Box::pin(shutdown);

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        let tls_acceptor = tls_acceptor.clone();
                        let http = http.clone();

                        tokio::spawn(async move {
                            match tls_acceptor.accept(stream).await {
                                Ok(tls_stream) => {
                                    debug!("TLS handshake successful");

                                    let service = service_fn(handle_request);
                                    let io = TokioIo::new(tls_stream);
                                    if let Err(e) = http.serve_connection(io, service).await {
                                        debug!("Connection error: {}", e);
                                    }
                                }
                                Err(e) => {
                                    debug!("TLS handshake error: {}", e);
                                }
                            }
                        });
                    }
                    Err(e) => {
                        debug!("Accept error: {}", e);
                    }
                }
            }
            _ = &mut shutdown_signal => {
                debug!("Shutting down HTTP/2 over TLS server...");
                break;
            }
        }
    }

    Ok(())
}
