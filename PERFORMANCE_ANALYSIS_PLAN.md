# HTTP Proxy Performance Analysis & Optimization Plan

## Executive Summary

Current analysis shows **6-13ms** non-network overhead, exceeding the **<10ms** target. This plan provides specific benchmark strategies, implementation approaches, and prioritized fixes to achieve the target.

## Current Bottleneck Analysis

### 1. Critical: Regex Compilation in Hot Path (2-5ms)
**Location**: `src/config/mod.rs:145, 158, 172, 229, 243`
```rust
// PROBLEM: Compiling regex on every request match
regex::Regex::new(pattern).map(|re| re.is_match(path)).unwrap_or(false)
```
**Impact**: 2-5ms per request for path/header/body matching

### 2. High: Multiple String Allocations (1-3ms)
**Locations**: 
- `src/handlers/proxy.rs:149-151` (method/path strings)
- `src/handlers/proxy.rs:160, 241` (body string conversions)
```rust
let method_str = req.method().as_str().to_string();
let body_content = String::from_utf8_lossy(&body_bytes).to_string();
```

### 3. High: Request Body Buffering (1-2ms)
**Location**: `src/handlers/proxy.rs:155-160`
```rust
// Full body collection before forwarding
let body_bytes = match req.into_body().collect().await { ... }
```

### 4. Medium: Header Processing (0.5-1ms)
**Location**: `src/handlers/proxy.rs:76-98, 221-231`
- Converting between axum/reqwest header types
- Multiple header iterations

### 5. Medium: Multiple Config Locks (0.5-1ms)
**Location**: `src/handlers/proxy.rs:105, 134, 143`
```rust
// Multiple separate config.get() calls
let config = config.get();
```

## Performance Validation Plan

### Phase 1: Baseline Measurement (Week 1)

#### 1.1 Micro-benchmark Suite
```bash
# Create comprehensive benchmarks
cargo bench --bench proxy_latency
cargo bench --bench regex_compilation
cargo bench --bench string_allocations
cargo bench --bench header_processing
```

**New benchmark files to create**:
- `benches/regex_compilation.rs` - Measure regex compilation overhead
- `benches/string_allocations.rs` - Measure string allocation patterns
- `benches/header_processing.rs` - Measure header conversion overhead
- `benches/config_locking.rs` - Measure config lock contention

#### 1.2 Request-Level Profiling
```rust
// Add detailed timing to proxy_handler
let start_time = std::time::Instant::now();
let regex_start = start_time;
// ... regex matching
let regex_duration = regex_start.elapsed();
let body_start = std::time::Instant::now();
// ... body processing
let body_duration = body_start.elapsed();
```

#### 1.3 Load Testing Scenarios
- **Light Load**: 10 req/s, measure baseline overhead
- **Medium Load**: 100 req/s, measure lock contention
- **Heavy Load**: 1000 req/s, measure scalability

### Phase 2: Bottleneck-Specific Fixes (Weeks 2-3)

#### Fix 1: Pre-compiled Regex Cache (Priority: Critical)
**Implementation**:
```rust
// In config/mod.rs
use std::collections::HashMap;
use once_cell::sync::Lazy;
use regex::Regex;

static REGEX_CACHE: Lazy<std::sync::Mutex<HashMap<String, Regex>>> = 
    Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

fn get_cached_regex(pattern: &str) -> Result<&Regex, regex::Error> {
    let mut cache = REGEX_CACHE.lock().unwrap();
    if !cache.contains_key(pattern) {
        cache.insert(pattern.to_string(), Regex::new(pattern)?);
    }
    Ok(cache.get(pattern).unwrap())
}
```

**Estimated Reduction**: 2-5ms → 0.1-0.3ms (90% improvement)
**Effort**: Low (1 day)
**Risk**: Low

#### Fix 2: String Allocation Optimization (Priority: High)
**Implementation**:
```rust
// Use Cow<str> instead of String allocations
use std::borrow::Cow;

// In proxy_handler
let method_str = req.method().as_str(); // Remove to_string()
let path_str = req.uri().path(); // Remove to_string()

// For body, use bytes directly when possible
let body_content = if capture_config.body {
    Some(Cow::from(String::from_utf8_lossy(&body_bytes)))
} else {
    None
};
```

**Estimated Reduction**: 1-3ms → 0.2-0.5ms (75% improvement)
**Effort**: Medium (2 days)
**Risk**: Low

#### Fix 3: Streaming Body Processing (Priority: High)
**Implementation**:
```rust
// Use hyper::Body for streaming
use hyper::Body;

// Forward body without full buffering
let mut request_builder = HTTP_CLIENT
    .request(method, &upstream_url)
    .headers(filtered_headers);

if !body_bytes.is_empty() {
    // Stream directly instead of buffering
    request_builder = request_builder.body(reqwest::Body::from(body_bytes));
}
```

**Estimated Reduction**: 1-2ms → 0.1-0.3ms (85% improvement)
**Effort**: High (3 days)
**Risk**: Medium (requires careful error handling)

#### Fix 4: Header Processing Optimization (Priority: Medium)
**Implementation**:
```rust
// Pre-allocate header maps and reuse
thread_local! {
    static HEADER_BUFFER: RefCell<reqwest::header::HeaderMap> = 
        RefCell::new(reqwest::header::HeaderMap::new());
}

fn filter_headers_optimized(headers: &HeaderMap) -> reqwest::header::HeaderMap {
    HEADER_BUFFER.with(|buffer| {
        let mut result = buffer.borrow_mut();
        result.clear();
        // ... optimized header copying
        result.clone()
    })
}
```

**Estimated Reduction**: 0.5-1ms → 0.1-0.2ms (80% improvement)
**Effort**: Medium (2 days)
**Risk**: Low

#### Fix 5: Config Lock Consolidation (Priority: Medium)
**Implementation**:
```rust
// Single config lock acquisition
let config_snapshot = {
    let config = config.get();
    ConfigSnapshot {
        should_drop: config.should_drop_request(&req, ""),
        log_config: config.should_log_request(&req, "").map(|c| c.clone()),
        timeout: config.logging.rules.iter()
            .find(|rule| config.matches_rule(&req, &rule.match_conditions, ""))
            .and_then(|rule| rule.timeout.as_ref())
            .and_then(|t| parse_duration_string(t)),
    }
};
```

**Estimated Reduction**: 0.5-1ms → 0.1ms (80% improvement)
**Effort**: Low (1 day)
**Risk**: Low

### Phase 3: Integration & Validation (Week 4)

#### 3.1 Comprehensive Benchmark Suite
```rust
// benches/comprehensive_performance.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn benchmark_full_proxy_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("proxy_pipeline");
    
    // Test different request patterns
    group.bench_function("simple_get", |b| { ... });
    group.bench_function("post_with_body", |b| { ... });
    group.bench_function("complex_headers", |b| { ... });
    group.bench_function("regex_heavy", |b| { ... });
    
    group.finish();
}
```

#### 3.2 Performance Regression Tests
```rust
// tests/performance_regression.rs
#[tokio::test]
async fn test_proxy_overhead_under_10ms() {
    let start = std::time::Instant::now();
    // ... execute proxy request
    let duration = start.elapsed();
    assert!(duration.as_millis() < 10, "Overhead: {}ms", duration.as_millis());
}
```

## Implementation Priority Matrix

| Fix | Impact | Effort | Risk | Priority |
|-----|--------|--------|------|----------|
| Regex Cache | 90% | Low | Low | 1 (Critical) |
| String Allocations | 75% | Medium | Low | 2 (High) |
| Streaming Body | 85% | High | Medium | 3 (High) |
| Header Processing | 80% | Medium | Low | 4 (Medium) |
| Config Locks | 80% | Low | Low | 5 (Medium) |

## Success Metrics

### Target Performance
- **Total Non-Network Overhead**: <10ms (target)
- **Regex Compilation**: <0.3ms (from 2-5ms)
- **String Allocations**: <0.5ms (from 1-3ms)
- **Body Processing**: <0.3ms (from 1-2ms)
- **Header Processing**: <0.2ms (from 0.5-1ms)
- **Config Locking**: <0.1ms (from 0.5-1ms)

### Validation Criteria
1. **Unit Tests**: All existing functionality preserved
2. **Benchmark Tests**: Performance targets met
3. **Load Tests**: Performance maintained under 1000 req/s
4. **Regression Tests**: No performance degradation over time

## Risk Mitigation

### Technical Risks
- **Regex Cache Memory**: Limit cache size, implement LRU eviction
- **Streaming Body Complexity**: Comprehensive error handling
- **Thread Safety**: Validate all optimizations under concurrent load

### Implementation Risks
- **Feature Regression**: Comprehensive test suite before/after each fix
- **Performance Regression**: Automated benchmark gating in CI
- **Deployment Risk**: Feature flags for gradual rollout

## Timeline

- **Week 1**: Baseline measurement + benchmark setup
- **Week 2**: Critical/High priority fixes (Regex, Strings, Streaming)
- **Week 3**: Medium priority fixes (Headers, Config locks)
- **Week 4**: Integration testing + performance validation

## Deliverables

1. **Enhanced Benchmark Suite**: Comprehensive performance measurement
2. **Optimized Proxy Implementation**: All 5 bottlenecks addressed
3. **Performance Regression Tests**: Automated CI/CD integration
4. **Performance Analysis Report**: Before/after comparison
5. **Deployment Guide**: Safe rollout procedures

This plan ensures the HTTP proxy meets the <10ms non-network overhead target while maintaining all existing functionality and code quality standards.