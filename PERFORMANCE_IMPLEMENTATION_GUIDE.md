# Performance Optimization Implementation Guide

This guide provides step-by-step instructions for implementing the performance optimizations outlined in the analysis plan.

## Phase 1: Regex Cache Implementation (Critical Priority)

### 1.1 Update Config Module

**File**: `src/config/mod.rs`

Replace regex compilation calls with cached versions:

```rust
// Replace this (line 145):
regex::Regex::new(pattern).map(|re| re.is_match(path)).unwrap_or(false)

// With this:
crate::performance::get_cached_regex(pattern)
    .map(|re| re.is_match(path))
    .unwrap_or(false)
```

**Apply to locations**:
- Line 145: Path pattern matching
- Line 158: Header pattern matching  
- Line 172: Body pattern matching
- Line 229: Response header matching
- Line 243: Response body matching

### 1.2 Add Performance Module

The performance module is already created at `src/performance.rs`. Add it to `lib.rs`:

```rust
pub mod performance;
```

## Phase 2: String Allocation Optimization (High Priority)

### 2.1 Update Proxy Handler

**File**: `src/handlers/proxy.rs`

Replace string allocations with string references:

```rust
// Replace lines 149-151:
let method_str = req.method().as_str().to_string();
let headers = req.headers().clone();
let req_path = req.uri().path().to_string();

// With:
let method_str = req.method().as_str(); // &str, no allocation
let headers = req.headers().clone(); // Keep this for header processing
let req_path = req.uri().path(); // &str, no allocation
```

### 2.2 Optimize Body Processing

```rust
// Replace line 160:
let body_content = String::from_utf8_lossy(&body_bytes).to_string();

// With conditional allocation:
let body_content = if log_request_config.as_ref().map(|c| c.body).unwrap_or(false) {
    Some(String::from_utf8_lossy(&body_bytes).to_string())
} else {
    None
};
```

## Phase 3: Config Lock Consolidation (Medium Priority)

### 3.1 Implement Config Snapshot

**File**: `src/handlers/proxy.rs`

Replace multiple config lock acquisitions with single snapshot:

```rust
// Replace lines 105, 134, 143:
let config_snapshot = {
    let config = config.get();
    crate::performance::ConfigSnapshot::from_config(&config, &req)
};

if let Some(drop_response) = config_snapshot.should_drop {
    // ... handle drop
}

// Use config_snapshot.log_config and config_snapshot.timeout
```

## Phase 4: Header Processing Optimization (Medium Priority)

### 4.1 Update Header Filtering

**File**: `src/handlers/proxy.rs`

Replace `filter_headers` function call:

```rust
// Replace line 179:
let filtered_headers = filter_headers(&headers);

// With:
let filtered_headers = crate::performance::filter_headers_optimized(&headers);
```

## Phase 5: Performance Monitoring Integration

### 5.1 Add Performance Timing

**File**: `src/handlers/proxy.rs`

Add performance monitoring to proxy handler:

```rust
#[axum::debug_handler]
pub async fn proxy_handler(State(config): State<Arc<ConfigHolder>>, req: Request) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let result = async {
        // ... existing proxy logic
    }.await;
    
    let overhead = start_time.elapsed();
    crate::performance::PERFORMANCE_METRICS.record_request(overhead);
    
    result
}
```

## Implementation Checklist

### Pre-Implementation
- [ ] Run baseline performance tests
- [ ] Document current performance metrics
- [ ] Create feature branch for optimizations

### Phase 1: Regex Cache
- [ ] Update config/mod.rs regex calls
- [ ] Add performance module to lib.rs
- [ ] Test regex cache functionality
- [ ] Verify performance improvement

### Phase 2: String Allocations
- [ ] Update proxy handler string usage
- [ ] Implement conditional body string allocation
- [ ] Test with various request types
- [ ] Verify memory usage reduction

### Phase 3: Config Locks
- [ ] Implement ConfigSnapshot usage
- [ ] Update all config access points
- [ ] Test under concurrent load
- [ ] Verify lock contention reduction

### Phase 4: Header Processing
- [ ] Update header filtering function
- [ ] Test with various header combinations
- [ ] Verify header processing performance

### Phase 5: Monitoring
- [ ] Add performance timing
- [ ] Implement metrics collection
- [ ] Test metrics accuracy
- [ ] Add performance endpoints

### Post-Implementation
- [ ] Run comprehensive performance tests
- [ ] Validate against targets
- [ ] Update documentation
- [ ] Create performance regression tests

## Testing Strategy

### Unit Tests
```bash
# Test individual optimizations
cargo test performance::tests

# Test regex cache specifically
cargo test regex_cache_performance

# Test config snapshot performance
cargo test config_snapshot_performance
```

### Benchmark Tests
```bash
# Run micro-benchmarks
cargo bench --bench performance_microbenchmarks

# Run comprehensive benchmarks
cargo bench --bench comprehensive_performance

# Compare with baseline
cargo bench --bench proxy_latency
```

### Load Tests
```bash
# Run performance validation script
./scripts/validate_performance.sh

# Manual load testing
for i in {1..100}; do curl -s "http://localhost:3000/httpbin.org/get" > /dev/null & done; wait
```

## Expected Performance Improvements

| Optimization | Current | Target | Expected Improvement |
|--------------|---------|--------|---------------------|
| Regex Compilation | 2-5ms | 0.1-0.3ms | 90% |
| String Allocations | 1-3ms | 0.2-0.5ms | 75% |
| Config Locking | 0.5-1ms | 0.1ms | 80% |
| Header Processing | 0.5-1ms | 0.1-0.2ms | 80% |
| **Total Overhead** | **6-13ms** | **<10ms** | **30-50%** |

## Rollback Plan

If any optimization causes issues:

1. **Feature Flags**: Use conditional compilation
2. **Gradual Rollout**: Deploy with monitoring
3. **Quick Revert**: Keep original code commented
4. **Performance Gates**: Automated checks in CI

## Monitoring and Alerting

### Key Metrics
- Request overhead (p50, p95, p99)
- Regex cache hit rate
- Memory usage trends
- Error rates

### Alert Thresholds
- p95 overhead > 15ms
- Regex cache hit rate < 80%
- Memory usage increase > 20%
- Error rate > 1%

This implementation guide ensures systematic, safe deployment of performance optimizations while maintaining system reliability.