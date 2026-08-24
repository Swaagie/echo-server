use http_body::Body as HttpBody;
use http_body_util::BodyExt;
use hyper::body::Bytes;
use hyper::header::{HeaderName, HeaderValue, CONTENT_LENGTH};
use hyper::{Method, Request, Response, StatusCode};
use log::debug;
use std::convert::Infallible;

const HOP_BY_HOP_HEADERS: [HeaderName; 9] = [
    hyper::header::CONNECTION,
    hyper::header::TE,
    hyper::header::TRAILER,
    hyper::header::TRANSFER_ENCODING,
    hyper::header::UPGRADE,
    hyper::header::PROXY_AUTHENTICATE,
    hyper::header::PROXY_AUTHORIZATION,
    HeaderName::from_static("keep-alive"),
    HeaderName::from_static("proxy-connection"),
];

pub fn format_headers(headers: &hyper::HeaderMap) -> String {
    let mut header_lines = Vec::new();
    for (name, value) in headers.iter() {
        if let Ok(value_str) = value.to_str() {
            header_lines.push(format!("{}: {}", name, value_str));
        } else {
            header_lines.push(format!("{}: <binary>", name));
        }
    }
    header_lines.join("\n")
}

pub async fn handle_request<B>(req: Request<B>) -> Result<Response<String>, Infallible>
where
    B: HttpBody<Data = Bytes> + Send + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    handle_request_with_alt_svc(req, None).await
}

pub async fn handle_request_with_alt_svc<B>(
    req: Request<B>,
    alt_svc: Option<HeaderValue>,
) -> Result<Response<String>, Infallible>
where
    B: HttpBody<Data = Bytes> + Send + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let headers = req.headers().clone();
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    debug!("Received {} request for {}", method, path);

    let (body_for_client, declared_len, status) = match (&method, path.as_str()) {
        (&Method::GET, "/") | (&Method::HEAD, "/") => {
            let echoed = format_headers(&headers);
            let len = echoed.len();
            if method == Method::HEAD {
                (String::new(), len, StatusCode::OK)
            } else {
                (echoed, len, StatusCode::OK)
            }
        }
        (&Method::POST, "/") | (&Method::PUT, "/") | (&Method::PATCH, "/") => {
            let body_bytes = match req.into_body().collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(e) => {
                    debug!("Failed to read request body: {}", e);
                    let mut response = Response::new(String::new());
                    *response.status_mut() = StatusCode::BAD_REQUEST;
                    response
                        .headers_mut()
                        .insert(CONTENT_LENGTH, HeaderValue::from_static("0"));
                    return Ok(response);
                }
            };
            let body_str = String::from_utf8_lossy(&body_bytes);

            let headers_str = format_headers(&headers);
            let combined_body = if body_str.is_empty() {
                headers_str
            } else {
                format!("{}\n\n{}", headers_str, body_str)
            };
            let len = combined_body.len();
            (combined_body, len, StatusCode::OK)
        }
        (&Method::OPTIONS, "/") => (String::new(), 0, StatusCode::OK),
        _ => (String::new(), 0, StatusCode::NOT_FOUND),
    };

    let mut response = Response::new(body_for_client);
    *response.status_mut() = status;

    for (name, value) in headers.iter() {
        if name == CONTENT_LENGTH || HOP_BY_HOP_HEADERS.contains(name) {
            continue;
        }
        response.headers_mut().insert(name.clone(), value.clone());
    }

    let content_length = HeaderValue::from_str(&declared_len.to_string())
        .unwrap_or_else(|_| HeaderValue::from_static("0"));
    response
        .headers_mut()
        .insert(CONTENT_LENGTH, content_length);

    if let Some(alt_svc) = alt_svc {
        response
            .headers_mut()
            .insert(hyper::header::ALT_SVC, alt_svc);
    }

    Ok(response)
}
