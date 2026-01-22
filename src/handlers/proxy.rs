use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use axum::extract::Request;
use crate::config::{ConfigHolder, ResponseCaptureConfig};
use http_body_util::BodyExt;
use serde_json;
use std::sync::Arc;
use tracing::info;
use once_cell::sync::Lazy;

#[derive(Debug)]
pub enum ProxyError {
    NoUpstreamUrl,
    InvalidUpstreamUrl(String),
    UpstreamRequestFailed(String),
    TimeoutError,
    BodyReadError,
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let (status, error_msg) = match self {
            ProxyError::NoUpstreamUrl => {
                let body = serde_json::json!({
                    "error": "No upstream URL provided",
                    "message": "Path must contain upstream URL after of first slash"
                });
                (StatusCode::BAD_REQUEST, body)
            }
            ProxyError::InvalidUpstreamUrl(url) => {
                let body = serde_json::json!({
                    "error": "Invalid upstream URL",
                    "url": url
                });
                (StatusCode::BAD_REQUEST, body)
            }
            ProxyError::UpstreamRequestFailed(msg) => {
                let body = serde_json::json!({
                    "error": "Upstream request failed",
                    "details": msg
                });
                (StatusCode::BAD_GATEWAY, body)
            }
            ProxyError::TimeoutError => {
                let body = serde_json::json!({
                    "error": "Upstream timeout"
                });
                (StatusCode::GATEWAY_TIMEOUT, body)
            }
            ProxyError::BodyReadError => {
                let body = serde_json::json!({
                    "error": "Failed to read request body"
                });
                (StatusCode::BAD_REQUEST, body)
            }
        };

        Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&error_msg).unwrap()))
            .unwrap()
    }
}

static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .build()
        .expect("Failed to create HTTP client")
});

fn filter_headers(headers: &HeaderMap) -> reqwest::header::HeaderMap {
    let mut result = reqwest::header::HeaderMap::new();
    
    // Hop-by-hop headers that should not be forwarded
    let hop_by_hop = [
        "connection", "keep-alive", "proxy-authenticate",
        "proxy-authorization", "te", "trailers", "transfer-encoding",
        "upgrade",
    ];
    
    for (name, value) in headers.iter() {
        let name_str = name.as_str();
        if !hop_by_hop.iter().any(|h| h.eq_ignore_ascii_case(name_str)) {
            if let Ok(key) = reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes()) {
                if let Ok(val) = reqwest::header::HeaderValue::from_bytes(value.as_bytes()) {
                    result.insert(key, val);
                }
            }
        }
    }
    
    result
}

#[axum::debug_handler]
pub async fn proxy_handler(State(config): State<Arc<ConfigHolder>>, req: Request) -> impl IntoResponse {
    let start_time = std::time::Instant::now();

    // Check if request should be dropped (acquire and release lock quickly)
    let drop_response = {
        let config = config.get();
        config.should_drop_request(&req, "")
    };

    if let Some(drop_response) = drop_response {
        let response = Response::builder()
            .status(drop_response.status_code)
            .body(Body::from(drop_response.body.unwrap_or_default()))
            .unwrap();

        if let Some(capture_config) = config.get().should_log_response(
            response.status().as_u16(),
            response.headers(),
            ""
        ) {
            log_response(&req, &response, capture_config, start_time.elapsed(), "");
        }

        return response;
    }

    let path = req.uri().path();
    let upstream_url = match extract_upstream_url(path) {
        Ok(url) => url,
        Err(e) => return e.into_response(),
    };

    // Get timeout from config before consuming the request (acquire and release lock quickly)
    let timeout = {
        let config = config.get();
        config.logging.rules.iter()
            .find(|rule| config.matches_rule(&req, &rule.match_conditions, ""))
            .and_then(|rule| rule.timeout.as_ref())
            .and_then(|t| parse_duration_string(t))
    };

    // Check if request should be logged (acquire and release lock quickly)
    let log_request_config = {
        let config = config.get();
        config.should_log_request(&req, "").map(|c| c.clone())
    };

    // Extract request details before consuming it
    let method_str = req.method().as_str().to_string();
    let headers = req.headers().clone();
    let req_path = req.uri().path().to_string();
    let req_method = req.method().clone();

    // Read request body
    let body_bytes = match req.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return ProxyError::BodyReadError.into_response(),
    };
    
    let body_content = String::from_utf8_lossy(&body_bytes).to_string();

    // Log request if configured
    if let Some(ref capture_config) = log_request_config {
        // Reconstruct a minimal request for logging
        let log_req = Request::builder()
            .method(req_method)
            .uri(&req_path)
            .body(Body::from(body_content.clone()))
            .unwrap();
        log_request(&log_req, capture_config, std::time::Duration::default(), &body_content, timeout);
    }

    // Build upstream request
    let method = match reqwest::Method::from_bytes(method_str.as_bytes()) {
        Ok(m) => m,
        Err(_) => return ProxyError::UpstreamRequestFailed("Invalid method".to_string()).into_response(),
    };

    let filtered_headers = filter_headers(&headers);

    let mut request_builder = HTTP_CLIENT
        .request(method, &upstream_url)
        .headers(filtered_headers);

    if !body_bytes.is_empty() {
        request_builder = request_builder.body(reqwest::Body::from(body_bytes.clone()));
    }

    if let Some(t) = timeout {
        request_builder = request_builder.timeout(t);
    }

    // Send request to upstream
    let upstream_resp = match request_builder.send().await {
        Ok(resp) => resp,
        Err(e) => {
            if e.is_timeout() {
                return ProxyError::TimeoutError.into_response();
            } else if e.is_connect() || e.is_request() {
                return ProxyError::UpstreamRequestFailed(e.to_string()).into_response();
            } else {
                return ProxyError::UpstreamRequestFailed(e.to_string()).into_response();
            }
        }
    };

    // Convert upstream response to axum response
    let status = StatusCode::from_u16(upstream_resp.status().as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    
    let mut response_builder = Response::builder()
        .status(status);

    // Forward response headers
    let hop_by_hop = [
        "connection", "keep-alive", "proxy-authenticate",
        "proxy-authorization", "te", "trailers", "transfer-encoding",
        "upgrade",
    ];

    let mut resp_headers = HeaderMap::new();
    for (name, value) in upstream_resp.headers() {
        let name_str = name.as_str();
        if !hop_by_hop.iter().any(|h| h.eq_ignore_ascii_case(name_str)) {
            if let Ok(header_name) = HeaderName::from_bytes(name_str.as_bytes()) {
                if let Ok(header_value) = HeaderValue::from_bytes(value.as_bytes()) {
                    resp_headers.insert(header_name.clone(), header_value.clone());
                    response_builder = response_builder.header(header_name, header_value);
                }
            }
        }
    }

    // Read response body
    let resp_body_bytes = match upstream_resp.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            return ProxyError::UpstreamRequestFailed(format!("Failed to read response body: {}", e)).into_response();
        }
    };

    let _response_body_str = String::from_utf8_lossy(&resp_body_bytes).to_string();

    // Build and return response
    response_builder
        .body(Body::from(resp_body_bytes))
        .unwrap()
}

pub fn extract_upstream_url(path: &str) -> Result<String, ProxyError> {
    let url_str = path.strip_prefix('/').ok_or(ProxyError::NoUpstreamUrl)?;
    
    if url_str.is_empty() {
        return Err(ProxyError::NoUpstreamUrl);
    }

    // Validate that it's a valid URL by parsing it
    if let Err(e) = url_str.parse::<reqwest::Url>() {
        return Err(ProxyError::InvalidUpstreamUrl(format!("{}: {}", url_str, e)));
    }

    Ok(url_str.to_string())
}

fn log_request(
    req: &Request,
    capture_config: &crate::config::CaptureConfig,
    duration: std::time::Duration,
    body_content: &str,
    timeout: Option<std::time::Duration>,
) {
    let mut log_entry = serde_json::json!({
        "type": "request",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    if capture_config.method {
        log_entry["method"] = req.method().as_str().into();
    }

    if capture_config.path {
        log_entry["path"] = req.uri().path().into();
    }

    if capture_config.timing {
        log_entry["duration_ms"] = (duration.as_millis() as u64).into();
    }

    if let Some(timeout) = timeout {
        log_entry["timeout_ms"] = (timeout.as_millis() as u64).into();
    }

    if !capture_config.headers.is_empty() {
        let mut headers_obj = serde_json::Map::new();
        for header_name in &capture_config.headers {
            if let Some(value) = req.headers().get(header_name) {
                if let Ok(value_str) = value.to_str() {
                    headers_obj.insert(header_name.clone(), value_str.into());
                }
            }
        }
        if !headers_obj.is_empty() {
            log_entry["headers"] = headers_obj.into();
        }
    }

    if capture_config.body {
        log_entry["body"] = body_content.into();
    }

    info!("{}", serde_json::to_string(&log_entry).unwrap_or_else(|_| "Failed to serialize request log".to_string()));
}

fn log_response(
    req: &Request,
    resp: &Response,
    capture_config: &ResponseCaptureConfig,
    duration: std::time::Duration,
    body_content: &str,
) {
    let mut log_entry = serde_json::json!({
        "type": "response",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    if capture_config.status_code {
        log_entry["status_code"] = resp.status().as_u16().into();
    }

    if capture_config.timing {
        log_entry["duration_ms"] = (duration.as_millis() as u64).into();
    }

    if !capture_config.headers.is_empty() {
        let mut headers_obj = serde_json::Map::new();
        for header_name in &capture_config.headers {
            if let Some(value) = resp.headers().get(header_name) {
                if let Ok(value_str) = value.to_str() {
                    headers_obj.insert(header_name.clone(), value_str.into());
                }
            }
        }
        if !headers_obj.is_empty() {
            log_entry["headers"] = headers_obj.into();
        }
    }

    if capture_config.body {
        log_entry["body"] = body_content.into();
    }

    log_entry["request_method"] = req.method().as_str().into();
    log_entry["request_path"] = req.uri().path().into();

    info!("{}", serde_json::to_string(&log_entry).unwrap_or_else(|_| "Failed to serialize response log".to_string()));
}

pub fn parse_duration_string(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    
    if s.is_empty() {
        return None;
    }

    if let Some(suffix) = s.strip_suffix("ms") {
        let num_str = suffix.trim();
        num_str.parse::<u64>().ok().map(std::time::Duration::from_millis)
    } else if let Some(suffix) = s.strip_suffix("s") {
        let num_str = suffix.trim();
        num_str.parse::<u64>().ok().map(std::time::Duration::from_secs)
    } else {
        None
    }
}
