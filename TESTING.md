# LogProx Manual Testing Guide

## Quick Start

### 1. Start the Server

```bash
# Default config
cargo run --release

# Or specify port
PORT=8080 cargo run --release

# With custom config
CONFIG_FILE=my-config.yaml cargo run --release
```

The server starts on `http://localhost:3000` by default.

---

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `3000` | Server listen port |
| `CONFIG_FILE` | `config.yaml` | Path to YAML config |
| `RUST_LOG` | `info` | Log level (debug, info, warn, error) |

---

## Minimal Working Config

Create `config.yaml`:

```yaml
logging:
  default: false

drop:
  default: false

response_logging:
  default: false
```

---

## API Endpoints

### Health Check

```bash
curl -s http://localhost:3000/health
```

**Expected Output:**
```
OK
```

**Status Code:** `200 OK`

---

### Get Configuration

```bash
curl -s http://localhost:3000/config | jq .
```

**Expected Output:** JSON with full configuration including:
- `server` - Server settings
- `logging` - Logging rules
- `drop` - Drop rules  
- `response_logging` - Response logging rules

**Status Code:** `200 OK`

---

### Reload Configuration

```bash
curl -s -X POST http://localhost:3000/config/reload
```

**Expected Output:**
```
Configuration reloaded successfully
```

**Status Code:** `200 OK` (or `500` on error)

---

### Configuration Documentation

```bash
curl -s http://localhost:3000/config/docs
```

**Expected Output:** Markdown documentation of configuration options

**Status Code:** `200 OK`

---

## Proxy Functionality

### Proxy GET Request

```bash
curl -s "http://localhost:3000/https://httpbin.org/get"
```

**Expected Output:** JSON response from httpbin.org with request details

**Status Code:** `200 OK` (or `502` if upstream unreachable)

---

### Proxy POST Request

```bash
curl -s -X POST "http://localhost:3000/https://httpbin.org/post" \
  -H "Content-Type: application/json" \
  -d '{"test": "data"}'
```

**Expected Output:** JSON response echoing back the POST data

**Status Code:** `200 OK`

---

### Proxy with Custom Headers

```bash
curl -s "http://localhost:3000/https://httpbin.org/headers" \
  -H "X-Custom-Header: test-value"
```

**Expected Output:** JSON showing custom header passed to upstream

---

## Testing with Logging Rules

### Config with Logging Rule

```yaml
logging:
  default: false
  rules:
    - name: "Log API requests"
      match_conditions:
        path:
          patterns:
            - "/api/.*"
      capture:
        method: true
        path: true
        timing: true
```

Start server with this config, then:

```bash
# Should be logged (matches /api/.*)
curl -s "http://localhost:3000/https://httpbin.org/api/users"

# Should NOT be logged
curl -s "http://localhost:3000/https://httpbin.org/get"
```

---

## Testing with Drop Rules

### Config with Drop Rule

```yaml
drop:
  default: false
  rules:
    - name: "Block bot traffic"
      match_conditions:
        headers:
          "user-agent": ".*bot.*"
      response:
        status_code: 403
        body: "Access denied - bots not allowed"
```

Test:

```bash
# Should be blocked
curl -s -A "bad-bot/1.0" "http://localhost:3000/https://httpbin.org/get"

# Expected: "Access denied - bots not allowed"
# Status: 403 Forbidden
```

---

## Error Cases

### Invalid Proxy URL

```bash
curl -s "http://localhost:3000/invalid-url"
```

**Expected:** Error response about invalid URL format  
**Status:** `400 Bad Request`

### Upstream Unreachable

```bash
curl -s "http://localhost:3000/https://localhost:99999/nonexistent"
```

**Expected:** Error about upstream connection failure  
**Status:** `502 Bad Gateway`

---

## Quick Test Script

```bash
#!/bin/bash
set -e

BASE="http://localhost:3000"

echo "=== LogProx Manual Test Suite ==="

echo -e "\n[1/7] Health check..."
curl -s "$BASE/health" | grep -q "OK" && echo "✓ Health OK" || echo "✗ Health FAILED"

echo -e "\n[2/7] Get config..."
curl -s "$BASE/config" | grep -q "logging" && echo "✓ Config OK" || echo "✗ Config FAILED"

echo -e "\n[3/7] Reload config..."
curl -s -X POST "$BASE/config/reload" | grep -q "success" && echo "✓ Reload OK" || echo "✗ Reload FAILED"

echo -e "\n[4/7] Proxy GET..."
curl -s "$BASE/https://httpbin.org/get" | grep -q "httpbin" && echo "✓ Proxy GET OK" || echo "✗ Proxy GET FAILED"

echo -e "\n[5/7] Proxy POST..."
curl -s -X POST "$BASE/https://httpbin.org/post" \
  -H "Content-Type: application/json" \
  -d '{"test":"data"}' | grep -q "application/json" && echo "✓ Proxy POST OK" || echo "✗ Proxy POST FAILED"

echo -e "\n[6/7] Proxy with headers..."
curl -s "$BASE/https://httpbin.org/headers" \
  -H "X-Custom: test" | grep -q "X-Custom" && echo "✓ Headers OK" || echo "✗ Headers FAILED"

echo -e "\n[7/7] Config docs..."
curl -s "$BASE/config/docs" | grep -q "Configuration" && echo "✓ Docs OK" || echo "✗ Docs FAILED"

echo -e "\n=== All Tests Complete ==="
```

---

## Troubleshooting

### Server Won't Start

```bash
# Check if port is in use
lsof -i :3000

# Try different port
PORT=3001 cargo run --release
```

### Config Errors

```bash
# Validate YAML syntax
yamllint config.yaml

# Check config with debug logging
RUST_LOG=debug cargo run --release 2>&1 | head -50
```

### Connection Issues

```bash
# Test upstream directly
curl -v https://httpbin.org/get

# Check network connectivity
ping httpbin.org
```
