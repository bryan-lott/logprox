# LogProx (Logging Proxy)

A passthrough HTTP proxy that conditionally logs and/or drops requests based on
rulesets before forwarding the request.

## Problem Statement

Accessing an external API with a deprecated version can cause additional cost,
bad data, and/or banning of access from the API. Tracking down where those
requests are coming from can be a huge headache.

LogProx offers a solution. Place it between any internal callers and the
external API, set up rules to log for specific headers, methods, paths, or
request bodies.

## Features

- [x] Primary Goal: exceptionally low latency overhead (tenths of millisecond)
- [x] Conditionally log request based on:
  - [x] Request Headers
  - [x] URL Path
  - [x] URL Method
  - [x] Request Body
- [x] Conditionally drop requests based on the above criteria
- [ ] Conditionally inject additional headers based on the above criteria
- [x] Reloading of the config file via POST request to LogProx (on-the-fly reloading)
- [x] GET endpoint returning the current config
- [x] GET endpoint returning the configuration documentation
- [x] Conditionally log responses

## Donation

If this is helpful in your day to day, please consider sending some of your hard earned dollars my way, thanks!!

[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/W7W31N7O4H)

## Configuration

LogProx uses a YAML configuration file to define logging and request dropping rules.
The configuration supports environment variable substitution using `${VAR_NAME}`
syntax.

### Environment Variables

- `PORT`: Server port (default: 3000)
- `CONFIG_FILE`: Path to config file (default: config.yaml)

### Configuration Structure

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

## API Endpoints

- `GET /health` - Health check
- `GET /config` - Current configuration (JSON)
- `GET /config/docs` - Configuration documentation
- `POST /config/reload` - Reload configuration from file

## Notes

- Configuration is loaded on startup and can be reloaded via POST /config/reload
- Invalid regex patterns will cause rule matching to fail for that condition
- Request bodies are consumed for all requests to enable body matching
- Response logging captures response details after proxy processing
- Environment variables are substituted at config load time
- All pattern matching is case-sensitive unless specified otherwise

## Authors

- [@bryan-lott](https://www.github.com/bryan-lott)

## License

[GNU GPLv3](https://choosealicense.com/licenses/gpl-3.0/)
