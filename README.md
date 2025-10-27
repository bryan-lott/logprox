# LogProx (Logging Proxy)

A passthrough HTTP proxy that conditionally logs requests based on a ruleset before forwarding the request.

## Problem Statement

Accessing an external API with a deprecated version can cause additional cost, bad data, and/or banning of access from the API. Tracking down where those requests are coming from can be a huge headache.

LogProx offers a solution. Place it between any internal callers and the external API, set up rules to log for specific headers, methods, paths, or (todo) request bodies.

## Features

- [x] Primary Goal: exceptionally low latency overhead, currently measured in tenths of a millisecond.
- [ ] Conditionally log request based on:
  - [x] Request Headers
  - [x] URL Path
  - [x] URL Method
  - [ ] Request Body
 - [x] Conditionally drop requests based on the above criteria
- [ ] Conditionally inject additional headers based on the above criteria
- [x] Reloading of the config file via POST request to LogProx (on-the-fly reloading)
- [ ] GET endpoint returning the current config
- [ ] GET endpoint returning the configuration documentation
- [ ] Conditionally log responses

## Authors

- [@bryan-lott](https://www.github.com/bryan-lott)

## License

[GNU GPLv3](https://choosealicense.com/licenses/gpl-3.0/)
