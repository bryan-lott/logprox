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
- [ ] GET endpoint returning the configuration documentation
- [ ] Conditionally log responses

## Configuration

LogProx can be configured via a YAML config file (default: `config.yaml`) and
environment variables.

### Environment Variables

- `PORT`: Server port (default: 3000)
- `CONFIG_FILE`: Path to config file (default: config.yaml)

### Config File

The config file supports environment variable substitution using `${VAR_NAME}` syntax. For example:

```yaml
drop:
  rules:
    - name: "API Key Required"
      response:
        status_code: 401
        body: "API Key ${API_KEY} required"
```

## Authors

- [@bryan-lott](https://www.github.com/bryan-lott)

## License

[GNU GPLv3](https://choosealicense.com/licenses/gpl-3.0/)
