# Changelog

## [0.3.0] - 2026-04-16

### Added
- **SSRF protection** — new `upstream:` config section (`UpstreamConfig`) with scheme allowlist,
  host allowlist/denylist, and private/loopback IP blocking (enabled by default).
- **Response logging** — responses are now logged when `response_logging` rules match.
  Previously the response logging config was parsed but never acted on.
- **Body size cap** — requests larger than 10 MB return 413 instead of risking OOM.
- **Regex pre-warming** — all patterns compiled at startup so first request pays no cost.
- **Pattern validation** — invalid regex in config is caught at load time, not at runtime.

### Changed
- Drop rules now evaluate **before** URL extraction, so they apply to any request path
  (including malformed ones), not just valid upstream URLs.
- `parking_lot::RwLock` replaces `std::sync::RwLock` — eliminates lock poisoning.
- `reqwest` upgraded 0.11 → 0.12, eliminating duplicate `hyper`/`http` in the dep tree.
- `serde_yaml` replaced with `serde_norway` (maintained fork of the deprecated 0.9 crate).
- `once_cell::sync::Lazy` replaced with `std::sync::LazyLock` (stable since Rust 1.80).
- Hop-by-hop headers filtered from both request and response (previously only request).
- Error responses never echo back the upstream URL (prevents information leakage).

### Removed
- Unused dependencies: `arc-swap`, `lru`, `object-pool`, `futures-core`, `tokio-stream`,
  `smallvec`, `bytes`, `http-body-util`, `pprof`, `iai`, `urlencoding`, `tokio-test`.

### Fixed
- MSRV declared as `rust-version = "1.80"` in `Cargo.toml`.

---

## [0.2.0] - Initial public release

Full request logging, request dropping, and response logging with regex-based rules.
