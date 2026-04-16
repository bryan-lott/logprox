use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use axum::extract::Request;
use crate::config::{CaptureConfig, ConfigHolder, ResponseCaptureConfig};
use std::sync::Arc;
use std::sync::LazyLock;
use tracing::info;

/// Maximum request body size accepted before returning 413. Prevents OOM via large uploads.
const MAX_BODY_SIZE: usize = 10 * 1024 * 1024; // 10 MB

/// Errors that can occur during proxying. Each variant maps to a distinct HTTP error response.
#[derive(Debug)]
pub enum ProxyError {
    NoUpstreamUrl,
    InvalidUpstreamUrl,
    BlockedUpstream,
    UpstreamRequestFailed(String),
    TimeoutError,
    BodyReadError,
    BodyTooLarge,
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let (status, error_msg) = match self {
            ProxyError::NoUpstreamUrl => (
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "error": "No upstream URL provided",
                    "message": "Path must contain upstream URL after the first slash"
                }),
            ),
            ProxyError::InvalidUpstreamUrl => (
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "Invalid upstream URL"}),
            ),
            ProxyError::BlockedUpstream => (
                StatusCode::FORBIDDEN,
                serde_json::json!({"error": "Upstream request blocked"}),
            ),
            ProxyError::UpstreamRequestFailed(msg) => (
                StatusCode::BAD_GATEWAY,
                serde_json::json!({"error": "Upstream request failed", "details": msg}),
            ),
            ProxyError::TimeoutError => (
                StatusCode::GATEWAY_TIMEOUT,
                serde_json::json!({"error": "Upstream timeout"}),
            ),
            ProxyError::BodyReadError => (
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": "Failed to read request body"}),
            ),
            ProxyError::BodyTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                serde_json::json!({"error": "Request body too large"}),
            ),
        };

        Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&error_msg).unwrap()))
            .unwrap()
    }
}

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .build()
        .expect("Failed to create HTTP client")
});

/// Hop-by-hop headers that must not be forwarded per RFC 7230 §6.1.
const HOP_BY_HOP: &[&str] = &[
    "connection", "keep-alive", "proxy-authenticate",
    "proxy-authorization", "te", "trailers", "transfer-encoding",
    "upgrade",
];

fn filter_headers(headers: &HeaderMap) -> reqwest::header::HeaderMap {
    let mut result = reqwest::header::HeaderMap::new();
    for (name, value) in headers.iter() {
        let name_str = name.as_str();
        if !HOP_BY_HOP.iter().any(|h| h.eq_ignore_ascii_case(name_str)) {
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

    // --- Extract request metadata before consuming the body ---
    let method_str = req.method().as_str().to_string();
    let headers = req.headers().clone();
    let req_path = req.uri().path().to_string();

    // --- Read body (with size cap) before any rule evaluation ---
    // Rules with body conditions need the real body to match correctly.
    let body_bytes = match axum::body::to_bytes(req.into_body(), MAX_BODY_SIZE).await {
        Ok(b) => b,
        Err(_) => return ProxyError::BodyTooLarge.into_response(),
    };
    let body_content = String::from_utf8_lossy(&body_bytes).to_string();

    // --- Drop check (with real body, before URL extraction so drop rules apply to all paths) ---
    let drop_response = {
        let cfg = config.get();
        cfg.should_drop_request_parts(&method_str, &req_path, &headers, &body_content)
    };

    if let Some(drop_resp) = drop_response {
        let response = Response::builder()
            .status(drop_resp.status_code)
            .body(Body::from(drop_resp.body.unwrap_or_default()))
            .unwrap();

        // Log the drop response if response_logging is configured
        let cfg = config.get();
        if let Some(capture) = cfg.should_log_response(response.status().as_u16(), response.headers(), "") {
            log_response(&method_str, &req_path, response.status().as_u16(), response.headers(), capture, start_time.elapsed(), "");
        }

        return response;
    }

    // --- Extract upstream URL (after drop check so drop rules apply to any path) ---
    let upstream_url = match extract_upstream_url(&req_path) {
        Ok(url) => url,
        Err(e) => return e.into_response(),
    };

    // --- SSRF validation ---
    {
        let cfg = config.get();
        if let Err(reason) = validate_upstream_ssrf(&upstream_url, &cfg.upstream) {
            tracing::warn!(upstream = %upstream_url, reason = %reason, "upstream blocked");
            return ProxyError::BlockedUpstream.into_response();
        }
    }

    // --- Get timeout and log config (with real body) ---
    let (timeout, log_request_config) = {
        let cfg = config.get();

        let timeout = cfg.logging.rules.iter()
            .find(|rule| cfg.matches_rule_parts(&method_str, &req_path, &headers, &body_content, &rule.match_conditions))
            .and_then(|rule| rule.timeout.as_deref().and_then(parse_duration_string));

        let log_cfg = cfg.should_log_request_parts(&method_str, &req_path, &headers, &body_content)
            .cloned();

        (timeout, log_cfg)
    };

    // --- Log request if configured ---
    if let Some(ref capture_config) = log_request_config {
        log_request(&method_str, &req_path, &headers, capture_config, std::time::Duration::default(), &body_content, timeout);
    }

    // --- Build and send upstream request ---
    let method = match reqwest::Method::from_bytes(method_str.as_bytes()) {
        Ok(m) => m,
        Err(_) => return ProxyError::UpstreamRequestFailed("Invalid method".to_string()).into_response(),
    };

    let filtered_headers = filter_headers(&headers);
    let mut request_builder = HTTP_CLIENT.request(method, &upstream_url).headers(filtered_headers);

    if !body_bytes.is_empty() {
        request_builder = request_builder.body(reqwest::Body::from(body_bytes));
    }
    if let Some(t) = timeout {
        request_builder = request_builder.timeout(t);
    }

    let upstream_resp = match request_builder.send().await {
        Ok(resp) => resp,
        Err(e) if e.is_timeout() => return ProxyError::TimeoutError.into_response(),
        Err(e) => return ProxyError::UpstreamRequestFailed(e.to_string()).into_response(),
    };

    // --- Build response, forwarding upstream headers ---
    let status = StatusCode::from_u16(upstream_resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let mut response_builder = Response::builder().status(status);
    let mut resp_headers = HeaderMap::new();

    for (name, value) in upstream_resp.headers() {
        let name_str = name.as_str();
        if !HOP_BY_HOP.iter().any(|h| h.eq_ignore_ascii_case(name_str)) {
            if let Ok(header_name) = HeaderName::from_bytes(name_str.as_bytes()) {
                if let Ok(header_value) = HeaderValue::from_bytes(value.as_bytes()) {
                    resp_headers.insert(header_name.clone(), header_value.clone());
                    response_builder = response_builder.header(header_name, header_value);
                }
            }
        }
    }

    let resp_body_bytes = match upstream_resp.bytes().await {
        Ok(b) => b,
        Err(e) => return ProxyError::UpstreamRequestFailed(
            format!("Failed to read response body: {}", e)
        ).into_response(),
    };

    // Decode response body only if response_logging is active (avoids allocation otherwise)
    let response_logging_active = {
        let cfg = config.get();
        cfg.response_logging.default || !cfg.response_logging.rules.is_empty()
    };
    let resp_body_content = if response_logging_active {
        String::from_utf8_lossy(&resp_body_bytes).into_owned()
    } else {
        String::new()
    };

    let final_resp = response_builder.body(Body::from(resp_body_bytes)).unwrap();

    // --- Log response if configured ---
    {
        let cfg = config.get();
        if let Some(capture) = cfg.should_log_response(
            final_resp.status().as_u16(),
            &resp_headers,
            &resp_body_content,
        ) {
            log_response(
                &method_str, &req_path,
                final_resp.status().as_u16(), &resp_headers,
                capture, start_time.elapsed(), &resp_body_content,
            );
        }
    }

    final_resp
}

pub fn extract_upstream_url(path: &str) -> Result<String, ProxyError> {
    let url_str = path.strip_prefix('/').ok_or(ProxyError::NoUpstreamUrl)?;

    if url_str.is_empty() {
        return Err(ProxyError::NoUpstreamUrl);
    }

    if url_str.parse::<reqwest::Url>().is_err() {
        return Err(ProxyError::InvalidUpstreamUrl);
    }

    Ok(url_str.to_string())
}

fn log_request(
    method: &str,
    path: &str,
    req_headers: &HeaderMap,
    capture_config: &CaptureConfig,
    duration: std::time::Duration,
    body_content: &str,
    timeout: Option<std::time::Duration>,
) {
    let mut log_entry = serde_json::json!({
        "type": "request",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    if capture_config.method {
        log_entry["method"] = method.into();
    }
    if capture_config.path {
        log_entry["path"] = path.into();
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
            if let Some(value) = req_headers.get(header_name) {
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
    req_method: &str,
    req_path: &str,
    resp_status: u16,
    resp_headers: &HeaderMap,
    capture_config: &ResponseCaptureConfig,
    duration: std::time::Duration,
    body_content: &str,
) {
    let mut log_entry = serde_json::json!({
        "type": "response",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "request_method": req_method,
        "request_path": req_path,
    });

    if capture_config.status_code {
        log_entry["status_code"] = resp_status.into();
    }
    if capture_config.timing {
        log_entry["duration_ms"] = (duration.as_millis() as u64).into();
    }
    if !capture_config.headers.is_empty() {
        let mut headers_obj = serde_json::Map::new();
        for header_name in &capture_config.headers {
            if let Some(value) = resp_headers.get(header_name) {
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

    info!("{}", serde_json::to_string(&log_entry).unwrap_or_else(|_| "Failed to serialize response log".to_string()));
}

fn is_private_ipv6(ip: std::net::Ipv6Addr) -> bool {
    let b = ip.octets();
    ip.is_loopback()               // ::1
    || ip.is_unspecified()         // ::
    || (b[0] & 0xfe) == 0xfc      // fc00::/7 unique-local
    || (b[0] == 0xfe && (b[1] & 0xc0) == 0x80) // fe80::/10 link-local
}

/// Returns Err with a reason string if the upstream URL should be blocked.
fn validate_upstream_ssrf(
    url_str: &str,
    cfg: &crate::config::UpstreamConfig,
) -> Result<(), &'static str> {
    let url = url_str.parse::<reqwest::Url>().map_err(|_| "invalid URL")?;

    // Scheme check
    if !cfg.allowed_schemes.iter().any(|s| s.eq_ignore_ascii_case(url.scheme())) {
        return Err("scheme not allowed");
    }

    let host_str = url.host_str().ok_or("no host")?;

    // Allowlist: if set, host must be in it (allowlist takes priority)
    if !cfg.allowed_hosts.is_empty() {
        return if cfg.allowed_hosts.iter().any(|h| h == host_str) {
            Ok(())
        } else {
            Err("host not in allowlist")
        };
    }

    // Denylist
    if cfg.denied_hosts.iter().any(|h| h == host_str) {
        return Err("host explicitly denied");
    }

    // Private-network block (literal IPs only; domain names require DNS to verify)
    if !cfg.allow_private_networks {
        if let Ok(ipv4) = host_str.parse::<std::net::Ipv4Addr>() {
            if ipv4.is_loopback() || ipv4.is_private() || ipv4.is_link_local() || ipv4.is_unspecified() {
                return Err("private/loopback address blocked");
            }
        } else if let Ok(ipv6) = host_str.parse::<std::net::Ipv6Addr>() {
            if is_private_ipv6(ipv6) {
                return Err("private/loopback address blocked");
            }
        }
        // Domain → allow; attacker can bypass via DNS rebinding (documented limitation)
    }

    Ok(())
}

pub fn parse_duration_string(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(suffix) = s.strip_suffix("ms") {
        suffix.trim().parse::<u64>().ok().map(std::time::Duration::from_millis)
    } else if let Some(suffix) = s.strip_suffix('s') {
        suffix.trim().parse::<u64>().ok().map(std::time::Duration::from_secs)
    } else {
        None
    }
}
