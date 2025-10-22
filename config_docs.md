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

## API Endpoints
- GET /health - Health check endpoint
- GET /config - Returns current configuration
- GET /config/docs - Returns this documentation
- POST /config/reload - Reloads configuration from file