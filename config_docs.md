# LogProx Configuration Documentation

## Overview
LogProx uses a YAML configuration file to define logging and request dropping rules. The configuration supports environment variable substitution using `${VAR_NAME}` syntax.

## Configuration Structure

### Server Configuration
```yaml
server:
  port: 3000                    # Server port (can be overridden by PORT env var)
  config_file: config.yaml      # Config file path (can be overridden by CONFIG_FILE env var)
```

## Configuration Structure

### Response Logging Configuration
```yaml
response_logging:
  # Default behavior if no rules match
  default: false

  # Rules for what responses to log, processed in order
  rules:
    - name: "Log error responses"
      match_conditions:
        # Match responses by status code
        status_codes:
          - 400
          - 401
          - 403
          - 404
          - 500
        # Match responses by headers (regex)
        headers:
          "content-type": "application/json.*"
        # Match responses by body content (regex)
        body:
          patterns:
            - "error.*"
      capture:
        # What to capture in logs when rule matches
        headers:
          - "content-type"
          - "x-request-id"
        body: true
        status_code: true
        timing: true
```

## API Endpoints
- GET /health - Health check endpoint
- GET /config - Returns current configuration
- GET /config/docs - Returns this documentation
- POST /config/reload - Reloads configuration from file