use std::convert::Infallible;
use hyper::header::{HeaderValue, CONTENT_LENGTH};
use hyper::{Method, Request, Response, StatusCode};
use http_body_util::BodyExt;
use hyper::body::Bytes;
use http_body::Body as HttpBody;
use log::debug;

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

pub async fn handle_request<B>(
    req: Request<B>,
) -> Result<Response<String>, Infallible>
where
    B: HttpBody<Data = Bytes> + Send + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    // Extract headers before consuming the request
    let headers = req.headers().clone();
    let method = req.method();
    let path = req.uri().path();

    debug!("Received {} request for {}", method, path);

    let mut response = Response::new(String::new());

    // Handle each HTTP verb - compute response body and status
    let (response_body, status) = match (method, path) {
        (&Method::GET, "/") => {
            // Return all request headers as the response body
            (Some(format_headers(&headers)), StatusCode::OK)
        }
        (&Method::POST, "/") | (&Method::PUT, "/") | (&Method::PATCH, "/") => {
            // Read the request body
            let body_bytes = req.into_body().collect().await.unwrap().to_bytes();
            let body_str = String::from_utf8_lossy(&body_bytes);

            // Format headers and combine with body
            let headers_str = format_headers(&headers);
            let combined_body = if body_str.is_empty() {
                headers_str
            } else {
                format!("{}\n\n{}", headers_str, body_str)
            };
            (Some(combined_body), StatusCode::OK)
        }
        (&Method::OPTIONS, "/") => {
            (None, StatusCode::OK)
        }
        _ => {
            (None, StatusCode::NOT_FOUND)
        }
    };

    // Set status code
    *response.status_mut() = status;

    // Set response body and content length if we have a body
    if let Some(body) = response_body {
        let content_length = HeaderValue::from_str(&body.len().to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0"));
        response.headers_mut().insert(CONTENT_LENGTH, content_length);
        *response.body_mut() = body;
    }

    Ok(response)
}

