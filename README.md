# LogProx 🏗️

> HTTP proxy with conditional logging and request control

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange)](https://www.rust-lang.org/)

## 🚀 Quick Start

### Installation

```bash
# Build from source
git clone https://github.com/bryan-lott/logprox.git
cd logprox
cargo build --release
```

### Basic Usage

```bash
# Start with default config
./target/release/logprox

# Specify custom config
./target/release/logprox --config my-config.yaml

# Set environment variables
PORT=8080 CONFIG_FILE=config.yaml ./target/release/logprox
```

### Simple Example

Create `config.yaml`:

```yaml
logging:
  default: false
  rules:
    - name: "Monitor API calls"
      match_conditions:
        path:
          patterns: ["/api/.*"]
      capture:
        method: true
        path: true
        timing: true
```

Start LogProx and test:

```bash
# Proxy format: http://host:port/https://upstream-domain/path
curl -X GET "http://localhost:3000/https://httpbin.org/api/test"
```

## ✨ Features

- **📝 Conditional Logging**: Log requests based on path, method, headers, body
- **🛡️ Request Control**: Drop requests based on configurable rules
- **🔄 Hot Reload**: Update configuration without restarting
- **📊 Built-in Monitoring**: Health checks and configuration endpoints

## 🏗️ Architecture

```
┌─────────────┐    ┌───────────┐    ┌────────────────┐
│   Client    │───▶│  LogProx  │───▶│  Upstream API  │
│ Application │    │           │    │                │
└─────────────┘    └───────────┘    └────────────────┘
                          │
                          ▼
                    ┌────────────┐
                    │   Logs &   │
                    │  Metrics   │
                    └────────────┘
```

## ⚙️ Configuration

LogProx uses YAML configuration with environment variable support (`${VAR_NAME}`).

### Environment Variables

| Variable      | Default       | Description             |
| ------------- | ------------- | ----------------------- |
| `PORT`        | `3000`        | Server port             |
| `CONFIG_FILE` | `config.yaml` | Configuration file path |

### Quick Reference

```yaml
logging:
  default: false
  rules:
    - name: "API Monitoring"
      match_conditions:
        path: { patterns: ["/api/.*"] }
        methods: ["POST", "PUT"]
      capture: { method: true, path: true, timing: true }

drop:
  default: false
  rules:
    - name: "Block Bots"
      match_conditions:
        headers: { "user-agent": ".*bot.*" }
      response: { status_code: 403, body: "Access denied" }

response_logging:
  default: false
  rules:
    - name: "Log Errors"
      match_conditions:
        status_codes: [400, 401, 403, 404, 500, 502, 503]
      capture: { status_code: true, timing: true }
```

### Request Format

Proxy requests use URL path encoding:

```
http://localhost:3000/https://api.example.com/users/123
                    ↑
                    Encoded upstream URL
```

### Pattern Matching

**Regex Examples:**

- `.*` - Match any characters
- `^/api/` - Match paths starting with /api/
- `\d+` - Match one or more digits
- `(option1|option2)` - Match either option

**Matching Logic:**

- Path patterns: At least one must match
- Methods: Must be in methods list (if specified)
- Headers: All specified headers must match
- Body patterns: At least one must match
- Rule evaluation: First match wins

## 🔌 API Reference

| Endpoint         | Method | Response                    |
| ---------------- | ------ | --------------------------- |
| `/health`        | GET    | `200 OK` with body `"OK"`   |
| `/config`        | GET    | Current JSON configuration  |
| `/config/docs`   | GET    | Configuration documentation |
| `/config/reload` | POST   | Reload configuration        |

### Usage Examples

```bash
# Health check
curl http://localhost:3000/health

# Get current config
curl http://localhost:3000/config | jq .

# Reload configuration
curl -X POST http://localhost:3000/config/reload

# View documentation
curl http://localhost:3000/config/docs
```

## 🔧 Troubleshooting

### Configuration Issues

```bash
# Validate YAML syntax
yamllint config.yaml

# Enable debug logging
RUST_LOG=debug ./target/release/logprox
```

### Common Problems

- **Invalid proxy format**: Use `/https://domain/path` format
- **Regex errors**: Test patterns with regex debugger
- **Permission denied**: Check config file permissions

## 🏎️ Performance

### Per-Request Latency

| Metric                     | Time     |
| -------------------------- | -------- |
| **Average proxy overhead** | **~3ms** |
| Direct upstream (local)    | 22ms     |
| Proxied request (local)    | 25ms     |

### Running Benchmarks

```bash
# Micro-benchmarks
cargo bench --bench performance_microbenchmarks

# Comprehensive benchmarks
cargo bench --bench comprehensive_performance
```

### Micro-benchmark Highlights

| Operation                     | Time     |
| ----------------------------- | -------- |
| Cached regex lookup           | 10-27 ns |
| Header iteration (6 headers)  | 15 ns    |
| Config lock (single)          | 14 ns    |
| String operations (optimized) | 17 ns    |
| YAML config parsing           | 10 µs    |

## 📄 License

**GNU GPLv3** © [Bryan Lott](https://github.com/bryan-lott)
