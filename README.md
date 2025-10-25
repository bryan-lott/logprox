# LogProx 🏗️

> A blazing-fast HTTP proxy with conditional logging and request control

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange)](https://www.rust-lang.org/)

## 🙏 Support the Project

If LogProx helps your team, consider supporting development:

[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/W7W31N7O4H)

**⚡ Exceptionally low latency** • **🔍 Conditional logging** • **🛡️ Request filtering** • **🔄 Hot reload**

[Quick Start](#-quick-start) • [Features](#-features) • [Configuration](#-configuration) • [Examples](#-examples)

## 🚀 Quick Start

### Installation

```bash
# Install from crates.io
cargo install logprox

# Or build from source
git clone https://github.com/bryan-lott/logprox.git
cd logprox
cargo build --release
```

### Basic Usage

```bash
# Start with default config
./target/release/logprox

# Or specify custom config
./target/release/logprox --config my-config.yaml

# Set environment variables
PORT=8080 CONFIG_FILE=config.yaml ./target/release/logprox
```

### Simple Example

Create a `config.yaml`:

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

Start LogProx and make a request:

```bash
curl -X GET "http://localhost:3000/api/test"
```

## 📋 Table of Contents

- [Problem Statement](#-problem-statement)
- [Features](#-features)
- [Architecture](#-architecture)
- [Performance](#-performance)
- [Configuration](#-configuration)
- [Examples](#-examples)
- [API Reference](#-api-reference)
- [Troubleshooting](#-troubleshooting)
- [Contributing](#-contributing)
- [License](#-license)

## ❓ Problem Statement

Accessing an external API with a deprecated version can cause additional cost,
bad data, and/or banning of access from the API. Tracking down where those
requests are coming from can be a huge headache.

**LogProx offers a solution**: Place it between any internal callers and the
external API, set up rules to log for specific headers, methods, paths, or
request bodies.

## ✨ Features

- **⚡ Ultra-Low Latency**: Sub-millisecond overhead for maximum performance
- **🔍 Smart Logging**: Conditional request/response logging based on flexible rules
- **🛡️ Request Control**: Drop, filter, and transform requests before they reach upstream services
- **🔄 Hot Reload**: Update configuration without restarting the service
- **📊 Built-in Monitoring**: Health checks, configuration endpoints, and response logging

### Feature Status

- [x] **Request Logging**: Headers, URL path, HTTP method, request body
- [x] **Request Dropping**: Block requests based on any criteria
- [x] **Response Logging**: Monitor upstream service responses
- [x] **Configuration Management**: Hot reload, validation, and documentation
- [ ] **Header Injection**: Add/modify headers conditionally
- [ ] **Rate Limiting**: Token bucket and sliding window algorithms
- [ ] **Load Balancing**: Distribute traffic across multiple upstream targets

## 🏗️ Architecture

LogProx sits between your application and upstream APIs, providing transparent proxying with intelligent request processing:

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

**Request Flow:**

1. **Receive**: Accept incoming HTTP requests
2. **Evaluate**: Check against logging and dropping rules
3. **Process**: Log, drop, or forward based on rules
4. **Monitor**: Capture response details if configured
5. **Respond**: Return results to client

## ⚡ Performance

**Benchmark Results** (on standard hardware):

- **Request Latency Overhead**: < 0.1ms per request
- **Throughput**: 10,000+ requests per second
- **Memory Usage**: ~5MB baseline + ~1KB per active connection
- **CPU Usage**: Minimal overhead (< 1% on modern hardware)

**Performance Philosophy:**

- Zero-copy request processing where possible
- Efficient regex compilation and caching
- Minimal allocations in hot paths
- Async I/O for maximum concurrency

## 🔮 Roadmap

We're actively working on these features. Have a suggestion? [Open an issue!](https://github.com/bryan-lott/logprox/issues)

### Phase 1 (Next Release)

- [ ] **Header Injection**: Add/modify headers conditionally
- [ ] **Configuration Validation**: Schema validation and rule testing
- [ ] **Metrics & Monitoring**: Prometheus/Open Telemetry integration

### Phase 2 (Future Releases)

- [ ] **Rate Limiting**: Token bucket algorithm with configurable limits
- [ ] **Load Balancing**: Round-robin and least-connections algorithms
- [ ] **Circuit Breaker**: Automatic failure detection and recovery
- [ ] **Request Transformation**: JSON path-based request/response modification

### Long-term Vision

- [ ] **Service Discovery**: Kubernetes, Consul, and etcd integration
- [ ] **Advanced Security**: IP filtering, API keys, and audit trails

## ⚙️ Configuration

LogProx uses YAML configuration files with support for environment variable substitution (`${VAR_NAME}`).

### Environment Variables

| Variable      | Default       | Description                |
| ------------- | ------------- | -------------------------- |
| `PORT`        | `3000`        | Server port to listen on   |
| `CONFIG_FILE` | `config.yaml` | Path to configuration file |

### Quick Reference

```yaml
server:
  port: 3000
  config_file: config.yaml

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

### 📚 Full Configuration Reference

#### Server Configuration

```yaml
server:
  port: 3000 # Server port (can be overridden by PORT env var)
  config_file: config.yaml # Config file path (can be overridden by CONFIG_FILE env var)
```

#### Logging Configuration

```yaml
logging:
  default: false # Default logging behavior if no rules match
  rules: # Array of logging rules
    - name: "Rule Name" # Descriptive name for the rule
      match_conditions: # Conditions that must ALL match
        path: # URL path patterns (regex)
          patterns:
            - "/api/.*"
        methods: # HTTP methods to match
          - "POST"
          - "PUT"
        headers: # Required headers and regex patterns
          "content-type": "application/json.*"
          "authorization": "Bearer .*"
        body: # Request body patterns (regex)
          patterns:
            - '"amount":\s*\d+'
      capture: # What to include in logs
        headers: # List of header names to capture
          - "content-type"
          - "user-agent"
        body: true # Whether to log request body
        method: true # Whether to log HTTP method
        path: true # Whether to log URL path
        timing: true # Whether to log timing information
```

#### Drop Configuration

```yaml
drop:
  default: false # Default drop behavior if no rules match
  rules: # Array of drop rules
    - name: "Rule Name" # Descriptive name for the rule
      match_conditions: # Conditions that must ALL match (same as logging)
        path:
          patterns:
            - "/deprecated/.*"
        methods:
          - "GET"
        headers:
          "user-agent": ".*bot.*"
        body:
          patterns:
            - "<script>.*</script>"
      response: # Response to return when dropping
        status_code: 403 # HTTP status code
         body: "Access denied" # Response body (supports env vars)
```

#### Response Logging Configuration

```yaml
response_logging:
  default: false # Default logging behavior if no rules match
  rules: # Array of response logging rules
    - name: "Log error responses" # Descriptive name for the rule
      match_conditions: # Conditions that must ALL match
        status_codes: # HTTP status codes to match
          - 400
          - 401
          - 403
          - 404
          - 500
        headers: # Required headers and regex patterns
          "content-type": "application/json.*"
        body: # Response body patterns (regex)
          patterns:
            - "error.*"
      capture: # What to include in logs
        headers: # List of header names to capture
          - "content-type"
          - "x-request-id"
        body: true # Whether to log response body
        status_code: true # Whether to log HTTP status code
        timing: true # Whether to log timing information
```

### Pattern Matching

#### Regex Syntax

All pattern matching uses Rust's regex engine. Common patterns:

- `.*` - Match any characters
- `^/api/` - Match paths starting with /api/
- `\d+` - Match one or more digits
- `(option1|option2)` - Match either option1 or option2

#### Matching Logic

- **Path patterns**: At least one pattern must match the request path
- **Methods**: The request method must be in the methods list (if specified)
- **Headers**: ALL specified headers must be present and match their patterns
- **Body patterns**: At least one pattern must match the request body content
- **Rule evaluation**: Rules are evaluated in order; first match wins

### Examples

#### Basic API Logging

```yaml
logging:
  default: false
  rules:
    - name: "Log API requests"
      match_conditions:
        path:
          patterns:
            - "/api/.*"
        methods:
          - "POST"
          - "PUT"
          - "DELETE"
      capture:
        headers:
          - "content-type"
          - "authorization"
        body: true
        method: true
        path: true
        timing: true
```

#### Security: Block Malicious Requests

```yaml
drop:
  default: false
  rules:
    - name: "Block XSS attempts"
      match_conditions:
        body:
          patterns:
            - "<script>.*</script>"
            - "javascript:"
            - "onload="
      response:
        status_code: 400
        body: "Malicious content detected"
```

#### Rate Limiting Simulation

```yaml
drop:
  default: false
  rules:
    - name: "Block bot traffic"
      match_conditions:
        headers:
          "user-agent": ".*(bot|crawler|spider).*"
      response:
        status_code: 429
        body: "Rate limit exceeded"
```

#### Response Monitoring

```yaml
response_logging:
  default: false
  rules:
    - name: "Log API errors"
      match_conditions:
        status_codes:
          - 400
          - 401
          - 403
          - 404
          - 500
          - 502
          - 503
      capture:
        headers:
          - "content-type"
          - "x-correlation-id"
        body: true
        status_code: true
        timing: true
```

## 🔌 API Reference

| Endpoint         | Method | Description                 | Response                  |
| ---------------- | ------ | --------------------------- | ------------------------- |
| `/health`        | GET    | Service health check        | `200 OK` with body `"OK"` |
| `/config`        | GET    | Current configuration       | `200 OK` with JSON config |
| `/config/docs`   | GET    | Configuration documentation | `200 OK` with Markdown    |
| `/config/reload` | POST   | Reload configuration        | `200 OK` or `500 Error`   |

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

### Common Issues

**Configuration Errors:**

```bash
# Validate your YAML syntax
yamllint config.yaml

# Test with verbose logging
RUST_LOG=debug ./logprox
```

**Performance Issues:**

- Check regex patterns for efficiency
- Monitor memory usage with `htop` or similar
- Review log volume and consider sampling

**Connection Problems:**

- Verify upstream service availability
- Check firewall rules and port accessibility
- Review timeout configurations

### Debug Mode

Enable detailed logging:

```bash
RUST_LOG=logprox=debug ./logprox
```

## 📝 Important Notes

- **Configuration**: Loaded on startup, hot-reloadable via API
- **Regex Patterns**: Invalid patterns cause rule matching to fail silently
- **Request Processing**: Bodies are consumed for all requests to enable matching
- **Response Logging**: Captures details after proxy processing completes
- **Environment Variables**: Substituted at config load time using `${VAR_NAME}` syntax
- **Pattern Matching**: Case-sensitive by default

## 🤝 Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## 📧 Support

- 🐛 **Bug Reports**: [GitHub Issues](https://github.com/bryan-lott/logprox/issues)
- 💡 **Feature Requests**: [GitHub Discussions](https://github.com/bryan-lott/logprox/discussions)

## 📄 License

**GNU GPLv3** © [Bryan Lott](https://github.com/bryan-lott)

---

<p align="center">
  <strong>Built with ❤️ in Rust</strong><br>
  A fast, reliable, and secure HTTP proxy for modern applications
</p>
