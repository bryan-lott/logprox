# LogProx Configuration Documentation

## Overview
LogProx uses a YAML configuration file to define logging, request dropping, and response logging rules. The configuration supports environment variable substitution using `${VAR_NAME}` syntax.

Environment variables:
- `CONFIG_FILE` — path to the config file (default: `config.yaml`)
- `PORT` — server port (default: `3000`)

## Configuration Structure

### Server Configuration
```yaml
server:
  port: 3000  # overridden by PORT env var
```

### Logging Configuration
```yaml
logging:
  default: false  # log all requests if no rules match

  rules:
    - name: "Log API calls"
      match_conditions:
        path:
          patterns: ["httpbin.org.*"]  # regex, at least one must match
        methods: ["POST", "PUT"]       # method must be in list (if specified)
        headers:
          "content-type": "application/json.*"  # all headers must match (regex)
        body:
          patterns: [".*"]             # regex, at least one must match
      capture:
        headers: ["content-type"]      # which request headers to log
        body: true
        method: true
        path: true
        timing: true
      timeout: 30s                     # per-request upstream timeout (e.g. 30s, 500ms)
```

### Drop Configuration
```yaml
drop:
  default: false  # drop all requests if no rules match

  rules:
    - name: "Block deprecated endpoint"
      match_conditions:
        path:
          patterns: ["/api/v1/deprecated.*"]
      response:
        status_code: 410
        body: "Gone. Use /api/v2."  # supports ${ENV_VAR} substitution
```

### Response Logging Configuration
```yaml
response_logging:
  default: false  # log all responses if no rules match

  rules:
    - name: "Log errors"
      match_conditions:
        status_codes: [400, 401, 403, 404, 500, 502, 503]
        headers:
          "content-type": "application/json.*"
        body:
          patterns: ["error.*"]
      capture:
        headers: ["content-type", "x-request-id"]
        body: true
        status_code: true
        timing: true
```

### Upstream Configuration (SSRF protection)
```yaml
upstream:
  allow_private_networks: false  # block 127.x, 10.x, 192.168.x, etc. (default: false)
  allowed_schemes: ["http", "https"]  # default
  allowed_hosts: []     # if non-empty, only these hosts are permitted (exact match)
  denied_hosts: []      # always blocked regardless of other settings
```

## Rule Matching Logic

- **Methods**: request method must appear in list (case-insensitive). Empty list = any method.
- **Path patterns**: regex. At least one must match. Empty list = any path.
- **Headers**: all specified headers must match their regex pattern.
- **Body patterns**: regex. At least one must match. Empty list = any body.
- **Rule evaluation**: first matching rule wins.

## API Endpoints
- `GET /health` — health check, returns `200 OK`
- `GET /config` — current configuration as JSON
- `GET /config/docs` — this documentation
- `POST /config/reload` — reload configuration from file
