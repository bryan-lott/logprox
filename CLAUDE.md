# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

LogProx is a blazing-fast HTTP proxy written in Rust that intercepts and processes requests based on configurable rules. Three core features:
1. **Conditional Logging**: Log requests matching pattern rules (by path, method, headers, body)
2. **Request Control**: Drop requests based on rules, returning custom responses
3. **Response Logging**: Log responses based on status codes and other conditions

Proxy format: `http://localhost:PORT/https://upstream-domain/path` — everything after the first `/` is the upstream URL.

## Build & Test Commands

```bash
# Build release binary (optimized)
cargo build --release

# Run tests (unit + integration)
cargo test

# Run specific test file
cargo test --test config_tests

# Run a single test
cargo test test_name -- --nocapture

# Run benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench performance_microbenchmarks

# Start development server (debug build)
cargo run

# Start with custom config and port
PORT=8080 CONFIG_FILE=custom.yaml cargo run
```

## Project Structure

```
src/
├── main.rs              # Entry point, server setup, router config
├── lib.rs               # Crate-level docs, module exports
├── config/
│   ├── mod.rs          # Config, ConfigHolder (RwLock wrapper), rule matching logic, regex cache
│   ├── request.rs      # LoggingConfig, DropConfig, MatchConditions, CaptureConfig
│   └── response.rs     # ResponseLoggingConfig, ResponseMatchConditions, ResponseCaptureConfig
├── handlers/
│   ├── mod.rs          # Module exports
│   ├── api.rs          # Health, config, reload endpoints
│   └── proxy.rs        # proxy_handler, SSRF validation, header filtering, error types

benches/                 # Criterion benchmarks
├── proxy_latency.rs    # End-to-end proxy latency (direct vs proxied)
├── performance_microbenchmarks.rs  # Micro-benchmarks (regex, headers, locks)
└── comprehensive_performance.rs    # Regex, header iteration, YAML parsing

tests/                   # Integration & unit tests
```

## Architecture Patterns

**Configuration System**: `Config` struct loaded from YAML via `serde_norway`, wrapped in `ConfigHolder` (`parking_lot::RwLock` for hot reload). Environment variables substituted via `${VAR_NAME}` in drop rule response bodies.

**Request Matching**: `matches_conditions_parts()` evaluates `MatchConditions`. Path/body patterns are regex (OR — any one match suffices). Method matching is case-insensitive. Headers are regex (AND — all must match). Different condition types are ANDed together.

**Proxy Handler Order**: body read → drop check → URL extract → SSRF validate → log request → forward → log response. Drop check runs before URL extraction so drop rules apply to any path (including malformed/non-URL paths).

**SSRF Protection**: `validate_upstream_ssrf` in `proxy.rs` runs before forwarding. Default policy: http/https only, private/loopback IPs blocked. Controlled via `upstream:` config section (`UpstreamConfig`). Set `allow_private_networks: true` when proxying to internal services. Benchmarks proxy localhost so they use `UpstreamConfig { allow_private_networks: true, ..Default::default() }`.

**Lazy Static HTTP Client**: `static HTTP_CLIENT: LazyLock<reqwest::Client>` — single global client, initialized on first use.

## Key Implementation Details

- **Regex Caching**: `static REGEX_CACHE: LazyLock<RwLock<HashMap<String, Arc<Regex>>>>` — compiled once, reused across all requests and threads. Pre-warmed at config load via `prewarm_regex_cache`. Fast path checks read lock first; slow path upgrades to write lock to insert.
- **Header Filtering**: `HOP_BY_HOP` const applied to both forwarded request headers and proxied response headers (RFC 7230 §6.1).
- **RwLock**: `parking_lot::RwLock` — no poisoning, faster than std. Readers only block on config reload writes.
- **Body Size Cap**: `MAX_BODY_SIZE = 10MB` — returns 413 if exceeded, prevents OOM.
- **Error Handling**: `ProxyError` enum with JSON responses. URL never echoed back in error messages (prevents info leakage).

## Testing Notes

- Unit tests in `tests/config_tests.rs` for rule matching logic
- Integration tests in `tests/integration_tests.rs` for end-to-end flow
- Proxy-specific tests in `tests/proxy_unit_tests.rs` for URL extraction and duration parsing

Use `--nocapture` flag to see test output.

## Performance Considerations

- Actual benchmark results: ~40µs direct (loopback), ~75µs proxied, ~35µs proxy overhead
- Micro-benchmarks track regex cache lookup, header iteration, lock contention, YAML parsing
- `parking_lot` used for faster locks with no poisoning risk

## Configuration

- Load from `CONFIG_FILE` env var (default: `config.yaml`)
- Hot reload via `POST /config/reload` endpoint
- Patterns are regex; evaluation order matters (first rule match wins)
- Default behaviors: `logging.default` and `drop.default` apply if no rules match
- Full config reference served at runtime via `GET /config/docs`
