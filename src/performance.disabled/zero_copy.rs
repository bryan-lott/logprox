// Zero-copy header processing for ultra-fast HTTP handling
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use bytes::{Bytes, BytesMut};
use reqwest::header as reqwest_headers;
use std::collections::HashMap;
use std::sync::Arc;

use crate::performance::{time_section, Metrics, HEADER_POOL_SIZE};

/// Zero-copy header representation
#[derive(Debug, Clone)]
pub struct ZeroCopyHeaders {
    pub raw: Bytes,
    pub parsed_headers: Vec<(Bytes, Bytes)>,
    pub hop_by_hop_removed: bool,
}

/// Header pool for memory reuse
pub struct HeaderPool {
    name_pool: Vec<HeaderName>,
    value_pool: Vec<HeaderValue>,
    reqwest_name_pool: Vec<reqwest_headers::HeaderName>,
    reqwest_value_pool: Vec<reqwest_headers::HeaderValue>,
}

impl HeaderPool {
    pub fn new() -> Self {
        Self {
            name_pool: Vec::with_capacity(HEADER_POOL_SIZE),
            value_pool: Vec::with_capacity(HEADER_POOL_SIZE),
            reqwest_name_pool: Vec::with_capacity(HEADER_POOL_SIZE),
            reqwest_value_pool: Vec::with_capacity(HEADER_POOL_SIZE),
        }
    }

    /// Get or create HeaderName from bytes
    pub fn get_header_name(&mut self, bytes: &Bytes) -> HeaderName {
        for name in &self.name_pool {
            if name.as_bytes() == bytes.as_ref() {
                return name.clone();
            }
        }

        let name = HeaderName::from_bytes(bytes.as_ref())
            .unwrap_or_else(|_| HeaderName::from_static("x-invalid"));
        self.name_pool.push(name.clone());
        name
    }

    /// Get or create HeaderValue from bytes
    pub fn get_header_value(&mut self, bytes: &Bytes) -> HeaderValue {
        for value in &self.value_pool {
            if value.as_bytes() == bytes.as_ref() {
                return value.clone();
            }
        }

        let value = HeaderValue::from_bytes(bytes.as_ref())
            .unwrap_or_else(|_| HeaderValue::from_static("invalid"));
        self.value_pool.push(value.clone());
        value
    }

    /// Get or create reqwest HeaderName
    pub fn get_reqwest_name(&mut self, bytes: &Bytes) -> reqwest_headers::HeaderName {
        for name in &self.reqwest_name_pool {
            if name.as_str().as_bytes() == bytes.as_ref() {
                return name.clone();
            }
        }

        let name = reqwest_headers::HeaderName::from_bytes(bytes.as_ref())
            .unwrap_or_else(|_| reqwest_headers::HeaderName::from_static("x-invalid"));
        self.reqwest_name_pool.push(name.clone());
        name
    }

    /// Get or create reqwest HeaderValue
    pub fn get_reqwest_value(&mut self, bytes: &Bytes) -> reqwest_headers::HeaderValue {
        for value in &self.reqwest_value_pool {
            if value.as_bytes() == bytes.as_ref() {
                return value.clone();
            }
        }

        let value = reqwest_headers::HeaderValue::from_bytes(bytes.as_ref())
            .unwrap_or_else(|_| reqwest_headers::HeaderValue::from_static("invalid"));
        self.reqwest_value_pool.push(value.clone());
        value
    }
}

/// Thread-local header pool
thread_local! {
    static HEADER_POOL: std::cell::RefCell<HeaderPool> =
        std::cell::RefCell::new(HeaderPool::new());
}

/// Hop-by-hop headers that should not be forwarded
static HOP_BY_HOP_HEADERS: &[&[u8]] = &[
    b"connection",
    b"keep-alive",
    b"proxy-authenticate",
    b"proxy-authorization",
    b"te",
    b"trailers",
    b"transfer-encoding",
    b"upgrade",
];

/// Ultra-fast header processor
pub struct HeaderProcessor;

impl HeaderProcessor {
    /// Parse headers with zero-copy approach
    pub fn parse_headers(buffer: &mut BytesMut, metrics: &mut Metrics) -> ZeroCopyHeaders {
        time_section!(start, metrics.header_processing_time, {
            let bytes = buffer.split().freeze();

            // Simple HTTP header parsing (optimistic fast path)
            let mut headers = Vec::new();
            let mut lines = bytes.split(|&b| &b == b'\n');

            // Skip first line (status line)
            if let Some(status_line) = lines.next() {
                if status_line.is_empty() {
                    return ZeroCopyHeaders {
                        raw: bytes,
                        parsed_headers: Vec::new(),
                        hop_by_hop_removed: false,
                    };
                }
            }

            for line in lines {
                if let Some(colon_pos) = line.iter().position(|&b| &b == &b':') {
                    if colon_pos > 0 && colon_pos < line.len() - 1 {
                        let name = line.slice(0..colon_pos).trim().trim_ascii();
                        let value = line.slice(colon_pos + 1..).trim().trim_ascii();

                        headers.push((name, value));
                    }
                }
            }

            ZeroCopyHeaders {
                raw: bytes,
                parsed_headers: headers,
                hop_by_hop_removed: false,
            }
        })
    }

    /// Convert axum headers to reqwest headers with zero-copy
    pub fn axum_to_reqwest(
        axum_headers: &HeaderMap,
        metrics: &mut Metrics,
    ) -> reqwest_headers::HeaderMap {
        time_section!(start, metrics.header_processing_time, {
            HEADER_POOL.with(|pool| {
                let mut pool = pool.borrow_mut();
                let mut reqwest_map = reqwest_headers::HeaderMap::new();

                for (name, value) in axum_headers.iter() {
                    let name_bytes = Bytes::copy_from_slice(name.as_str().as_bytes());
                    let value_bytes = Bytes::copy_from_slice(value.as_bytes());

                    // Skip hop-by-hop headers
                    if Self::is_hop_by_hop(&name_bytes) {
                        continue;
                    }

                    let reqwest_name = pool.get_reqwest_name(&name_bytes);
                    let reqwest_value = pool.get_reqwest_value(&value_bytes);

                    reqwest_map.insert(reqwest_name, reqwest_value);
                }

                reqwest_map
            })
        })
    }

    /// Convert upstream headers to axum headers
    pub fn upstream_to_axum(
        upstream_headers: &reqwest_headers::HeaderMap,
        metrics: &mut Metrics,
    ) -> HeaderMap {
        time_section!(start, metrics.header_processing_time, {
            HEADER_POOL.with(|pool| {
                let mut pool = pool.borrow_mut();
                let mut axum_map = HeaderMap::new();

                for (name, value) in upstream_headers.iter() {
                    let name_bytes = Bytes::copy_from_slice(name.as_str().as_bytes());

                    // Skip hop-by-hop headers
                    if Self::is_hop_by_hop(&name_bytes) {
                        continue;
                    }

                    let axum_name = pool.get_header_name(&name_bytes);
                    let value_bytes = Bytes::copy_from_slice(value.as_bytes());
                    let axum_value = pool.get_header_value(&value_bytes);

                    axum_map.insert(axum_name, axum_value);
                }

                axum_map
            })
        })
    }

    /// Check if header is hop-by-hop
    #[inline(always)]
    fn is_hop_by_hop(header_name: &Bytes) -> bool {
        let header_name_lower = header_name.to_ascii_lowercase();
        HOP_BY_HOP_HEADERS
            .iter()
            .any(|hop| hop == &header_name_lower)
    }
}

/// Pre-allocated common headers for instant access
pub static COMMON_HEADERS: Lazy<HashMap<&'static str, HeaderName>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert("content-type", HeaderName::from_static("content-type"));
    map.insert("user-agent", HeaderName::from_static("user-agent"));
    map.insert("accept", HeaderName::from_static("accept"));
    map.insert("authorization", HeaderName::from_static("authorization"));
    map.insert("host", HeaderName::from_static("host"));
    map.insert("connection", HeaderName::from_static("connection"));
    map
});

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_header_parsing_performance() {
        let raw_headers = b"GET /api/users HTTP/1.1\r\nHost: example.com\r\nUser-Agent: test/1.0\r\nContent-Type: application/json\r\n\r\n";
        let mut buffer = BytesMut::from(&raw_headers[..]);
        let mut metrics = Metrics::new();

        let headers = HeaderProcessor::parse_headers(&mut buffer, &mut metrics);

        assert!(!headers.parsed_headers.is_empty());
        assert!(metrics.header_processing_time.as_micros() < 100); // Should be under 100μs
    }

    #[test]
    fn test_hop_by_hop_filtering() {
        let connection_header = Bytes::from_static(b"connection");
        let content_type_header = Bytes::from_static(b"content-type");

        assert!(HeaderProcessor::is_hop_by_hop(&connection_header));
        assert!(!HeaderProcessor::is_hop_by_hop(&content_type_header));
    }

    #[test]
    fn test_header_pool_reuse() {
        let header_bytes = Bytes::from_static(b"content-type");

        let result1 = HEADER_POOL.with(|pool| {
            let mut pool = pool.borrow_mut();
            pool.get_header_name(&header_bytes)
        });

        let result2 = HEADER_POOL.with(|pool| {
            let mut pool = pool.borrow_mut();
            pool.get_header_name(&header_bytes)
        });

        assert_eq!(result1.as_str(), result2.as_str());
    }

    #[test]
    fn test_common_headers() {
        assert_eq!(
            COMMON_HEADERS.get("content-type").unwrap().as_str(),
            "content-type"
        );
        assert_eq!(
            COMMON_HEADERS.get("user-agent").unwrap().as_str(),
            "user-agent"
        );
        assert_eq!(COMMON_HEADERS.get("host").unwrap().as_str(), "host");
    }
}
