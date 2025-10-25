# Contributing to LogProx

Thank you for your interest in contributing to LogProx! 🎉

We welcome contributions from everyone. This document provides guidelines and information for contributors.

## 📋 Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Development Workflow](#development-workflow)
- [Testing](#testing)
- [Submitting Changes](#submitting-changes)
- [Code Style](#code-style)
- [Performance Guidelines](#performance-guidelines)

## 🤝 Code of Conduct

This project follows a code of conduct to ensure a welcoming environment for all contributors. By participating, you agree to:

- Be respectful and inclusive
- Focus on constructive feedback
- Accept responsibility for mistakes
- Show empathy towards other contributors
- Help create a positive community

## 🚀 Getting Started

### Prerequisites

- **Rust**: 1.70 or later ([install here](https://rustup.rs/))
- **Git**: For version control
- **Make** (optional): For convenience scripts

### Quick Setup

```bash
# Fork and clone the repository
git clone https://github.com/YOUR_USERNAME/logprox.git
cd logprox

# Install dependencies
cargo build

# Run tests
cargo test

# Start developing!
```

## 🛠️ Development Setup

### Local Development

```bash
# Build in debug mode
cargo build

# Build optimized release
cargo build --release

# Run with default config
cargo run

# Run with custom config
cargo run -- --config path/to/config.yaml
```

### Testing Configuration

Create a test configuration file:

```yaml
# test-config.yaml
logging:
  default: true
  rules:
    - name: "Test Rule"
      match_conditions:
        path:
          patterns: ["/test/.*"]
      capture:
        method: true
        path: true
        timing: true
```

## 🏗️ Project Structure

```
logprox/
├── src/
│   ├── config/          # Configuration handling
│   │   ├── mod.rs       # Main config structs
│   │   ├── request.rs   # Request-related config
│   │   └── response.rs  # Response-related config
│   ├── handlers/        # HTTP request handlers
│   │   ├── mod.rs       # Handler exports
│   │   ├── api.rs       # API endpoints
│   │   └── proxy.rs     # Proxy logic
│   ├── lib.rs           # Library interface
│   └── main.rs          # Application entry point
├── tests/               # Integration tests
│   ├── config_tests.rs  # Configuration tests
│   └── integration_tests.rs # End-to-end tests
├── benches/             # Performance benchmarks
├── config.yaml          # Default configuration
├── config_docs.md       # Configuration documentation
├── Cargo.toml           # Rust dependencies
└── README.md            # Project documentation
```

## 🔄 Development Workflow

### 1. Choose an Issue

- Check [GitHub Issues](https://github.com/bryan-lott/logprox/issues) for open tasks
- Look for issues labeled `good first issue` or `help wanted`
- Comment on the issue to indicate you're working on it

### 2. Create a Branch

```bash
# Create and switch to a feature branch
git checkout -b feature/your-feature-name

# Or for bug fixes
git checkout -b fix/issue-number-description
```

### 3. Make Changes

- Write clear, focused commits
- Test your changes thoroughly
- Update documentation if needed
- Follow the code style guidelines

### 4. Test Your Changes

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run benchmarks
cargo bench

# Check code formatting
cargo fmt --check

# Run linter
cargo clippy
```

## 🧪 Testing

### Unit Tests

Add unit tests for new functionality:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_feature() {
        // Test implementation
        assert_eq!(result, expected);
    }
}
```

### Integration Tests

Add integration tests in `tests/` directory:

```rust
#[tokio::test]
async fn test_feature_integration() {
    // Integration test implementation
}
```

### Performance Testing

Add benchmarks in `benches/` directory:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_my_feature(c: &mut Criterion) {
    c.bench_function("my_feature", |b| {
        b.iter(|| black_box(my_function()))
    });
}
```

## 📝 Submitting Changes

### Pull Request Process

1. **Update Documentation**: Ensure README and docs reflect your changes
2. **Write Tests**: Add tests for new functionality
3. **Update CHANGELOG**: Add entry for user-facing changes
4. **Squash Commits**: Combine related commits into logical units

### Commit Message Format

```
type(scope): description

[optional body]

[optional footer]
```

**Types:**

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation
- `style`: Code style changes
- `refactor`: Code refactoring
- `test`: Testing
- `chore`: Maintenance

**Examples:**

```
feat(config): add support for environment variable substitution
fix(proxy): resolve memory leak in request body handling
docs(readme): update installation instructions
```

### Pull Request Template

When creating a PR, include:

- **Description**: What changes and why
- **Testing**: How you tested the changes
- **Breaking Changes**: Any breaking changes
- **Screenshots**: UI changes (if applicable)

## 🎨 Code Style

### Rust Guidelines

- Follow the [official Rust style guide](https://doc.rust-lang.org/style-guide/)
- Use `rustfmt` for consistent formatting
- Address all `clippy` warnings
- Write idiomatic Rust code

### Naming Conventions

```rust
// Structs and enums: PascalCase
pub struct ConfigHolder { ... }

// Functions and methods: snake_case
pub fn should_log_request(&self, ...) -> bool { ... }

// Constants: SCREAMING_SNAKE_CASE
const DEFAULT_PORT: u16 = 3000;

// Modules: snake_case
pub mod request_config;
```

### Documentation

````rust
/// Brief description of what this function does.
///
/// # Arguments
/// * `param1` - Description of param1
/// * `param2` - Description of param2
///
/// # Returns
/// Description of return value
///
/// # Examples
/// ```
/// let result = my_function(arg1, arg2);
/// assert_eq!(result, expected);
/// ```
pub fn my_function(param1: Type1, param2: Type2) -> ReturnType {
    // Implementation
}
````

## ⚡ Performance Guidelines

LogProx prioritizes performance. Keep these principles in mind:

### Memory Management

- Minimize allocations in hot paths
- Use `Bytes` for request/response bodies
- Prefer stack allocation over heap when possible
- Avoid cloning large data structures

### Async Best Practices

- Use async/await consistently
- Avoid blocking operations in async contexts
- Use appropriate buffer sizes for I/O
- Consider connection pooling for upstream services

### Algorithm Complexity

- Prefer O(1) or O(log n) operations in hot paths
- Cache compiled regex patterns
- Use efficient data structures (HashMap vs Vec where appropriate)

### Benchmarking

- Add benchmarks for performance-critical code
- Monitor memory usage and CPU overhead
- Test with realistic data sizes
- Compare performance before/after changes

## 📞 Getting Help

- **GitHub Issues**: For bugs and feature requests
- **GitHub Discussions**: For questions and community support
- **Discord**: Real-time chat (coming soon)

## 🙏 Recognition

Contributors will be recognized in:

- CHANGELOG.md for their contributions
- GitHub's contributor insights
- Release notes for significant contributions

Thank you for contributing to LogProx! 🚀</content>
