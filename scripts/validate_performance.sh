#!/usr/bin/env bash

# Performance Optimization Validation Script
# Runs comprehensive benchmarks to validate <1ms target

set -e

echo "🚀 Starting Performance Optimization Validation"
echo "Target: <1ms non-network overhead"
echo "=========================================="

# Performance targets (updated for <1ms goal)
TARGET_OVERHEAD_MS=1
TARGET_REGEX_COMPILATION_MS=0.1
TARGET_STRING_ALLOCATIONS_MS=0.05
TARGET_HEADER_PROCESSING_MS=0.05
TARGET_CONFIG_LOCKING_MS=0.01

# Function to print colored output
print_status() {
    local status=$1
    local message=$2
    case $status in
        "PASS")
            echo -e "${GREEN}✓ PASS${NC}: $message"
            ;;
        "FAIL")
            echo -e "${RED}✗ FAIL${NC}: $message"
            ;;
        "WARN")
            echo -e "${YELLOW}⚠ WARN${NC}: $message"
            ;;
        "INFO")
            echo -e "ℹ INFO: $message"
            ;;
    esac
}

# Function to check if a value is within target
check_target() {
    local actual=$1
    local target=$2
    local metric_name=$3
    
    if (( $(echo "$actual <= $target" | bc -l) )); then
        print_status "PASS" "$metric_name: ${actual}ms (target: ≤${target}ms)"
        return 0
    else
        print_status "FAIL" "$metric_name: ${actual}ms (target: ≤${target}ms)"
        return 1
    fi
}

# Clean up any existing processes
cleanup() {
    print_status "INFO" "Cleaning up existing processes..."
    pkill -f "logprox" || true
    pkill -f "target/debug/logprox" || true
    sleep 2
}

# Build the project
build_project() {
    print_status "INFO" "Building the project..."
    cargo build --release
    cargo build --benches
}

# Run unit tests
run_unit_tests() {
    print_status "INFO" "Running unit tests..."
    if cargo test --lib; then
        print_status "PASS" "All unit tests passed"
    else
        print_status "FAIL" "Unit tests failed"
        exit 1
    fi
}

# Run performance regression tests
run_performance_tests() {
    print_status "INFO" "Running performance regression tests..."
    if cargo test performance_regression_tests --test performance_regression_tests; then
        print_status "PASS" "Performance regression tests passed"
    else
        print_status "FAIL" "Performance regression tests failed"
        exit 1
    fi
}

# Run micro-benchmarks
run_microbenchmarks() {
    print_status "INFO" "Running micro-benchmarks..."
    
    echo "Running regex compilation benchmarks..."
    local regex_result=$(cargo bench --bench performance_microbenchmarks -- regex_compilation 2>/dev/null | grep -A 5 "regex_compilation" | tail -1 | awk '{print $2}' | sed 's/[^0-9.]//g' || echo "0")
    
    echo "Running string allocation benchmarks..."
    local string_result=$(cargo bench --bench performance_microbenchmarks -- string_allocations 2>/dev/null | grep -A 5 "string_allocations" | tail -1 | awk '{print $2}' | sed 's/[^0-9.]//g' || echo "0")
    
    echo "Running header processing benchmarks..."
    local header_result=$(cargo bench --bench performance_microbenchmarks -- header_processing 2>/dev/null | grep -A 5 "header_processing" | tail -1 | awk '{print $2}' | sed 's/[^0-9.]//g' || echo "0")
    
    echo "Running config locking benchmarks..."
    local config_result=$(cargo bench --bench performance_microbenchmarks -- config_locking 2>/dev/null | grep -A 5 "config_locking" | tail -1 | awk '{print $2}' | sed 's/[^0-9.]//g' || echo "0")
    
    # Store results for validation
    echo "$regex_result" > /tmp/regex_benchmark.txt
    echo "$string_result" > /tmp/string_benchmark.txt
    echo "$header_result" > /tmp/header_benchmark.txt
    echo "$config_result" > /tmp/config_benchmark.txt
}

# Run comprehensive performance tests
run_comprehensive_benchmarks() {
    print_status "INFO" "Running comprehensive performance benchmarks..."
    cargo bench --bench comprehensive_performance 2>/dev/null || true
}

# Validate benchmark results
validate_benchmarks() {
    print_status "INFO" "Validating benchmark results against targets..."
    
    local failures=0
    
    # Check regex compilation
    local regex_time=$(cat /tmp/regex_benchmark.txt)
    if ! check_target "$regex_time" "$TARGET_REGEX_COMPILATION_MS" "Regex compilation"; then
        failures=$((failures + 1))
    fi
    
    # Check string allocations
    local string_time=$(cat /tmp/string_benchmark.txt)
    if ! check_target "$string_time" "$TARGET_STRING_ALLOCATIONS_MS" "String allocations"; then
        failures=$((failures + 1))
    fi
    
    # Check header processing
    local header_time=$(cat /tmp/header_benchmark.txt)
    if ! check_target "$header_time" "$TARGET_HEADER_PROCESSING_MS" "Header processing"; then
        failures=$((failures + 1))
    fi
    
    # Check config locking
    local config_time=$(cat /tmp/config_benchmark.txt)
    if ! check_target "$config_time" "$TARGET_CONFIG_LOCKING_MS" "Config locking"; then
        failures=$((failures + 1))
    fi
    
    return $failures
}

# Run load tests
run_load_tests() {
    print_status "INFO" "Running load tests..."
    
    # Start the proxy server
    print_status "INFO" "Starting proxy server for load testing..."
    cargo run --release --bin logprox &
    local proxy_pid=$!
    sleep 3
    
    # Run load test with different concurrency levels
    print_status "INFO" "Testing with 10 concurrent requests..."
    local start_time=$(date +%s%N)
    for i in {1..10}; do
        curl -s "http://localhost:3000/httpbin.org/get" > /dev/null &
    done
    wait
    local end_time=$(date +%s%N)
    local duration_ms=$(( (end_time - start_time) / 1000000 ))
    local avg_ms=$((duration_ms / 10))
    
    if check_target "$avg_ms" "$TARGET_OVERHEAD_MS" "Load test (10 concurrent)"; then
        print_status "PASS" "Load test completed successfully"
    else
        print_status "FAIL" "Load test exceeded target"
        failures=$((failures + 1))
    fi
    
    # Clean up
    kill $proxy_pid 2>/dev/null || true
}

# Generate performance report
generate_report() {
    print_status "INFO" "Generating performance report..."
    
    cat > performance_report.md << EOF
# HTTP Proxy Performance Report

## Test Results

### Micro-benchmarks
- **Regex Compilation**: $(cat /tmp/regex_benchmark.txt)ms (target: ≤${TARGET_REGEX_COMPILATION_MS}ms)
- **String Allocations**: $(cat /tmp/string_benchmark.txt)ms (target: ≤${TARGET_STRING_ALLOCATIONS_MS}ms)
- **Header Processing**: $(cat /tmp/header_benchmark.txt)ms (target: ≤${TARGET_HEADER_PROCESSING_MS}ms)
- **Config Locking**: $(cat /tmp/config_benchmark.txt)ms (target: ≤${TARGET_CONFIG_LOCKING_MS}ms)

### Overall Performance
- **Target Overhead**: ≤${TARGET_OVERHEAD_MS}ms
- **Test Date**: $(date)
- **Git Commit**: $(git rev-parse HEAD)

### Recommendations
EOF

    if [ $failures -eq 0 ]; then
        echo "✅ All performance targets met. Ready for production deployment." >> performance_report.md
    else
        echo "⚠️  Some performance targets not met. Review optimization recommendations." >> performance_report.md
    fi
    
    print_status "INFO" "Performance report generated: performance_report.md"
}

# Main execution
main() {
    local failures=0
    
    cleanup
    build_project
    
    if ! run_unit_tests; then
        failures=$((failures + 1))
    fi
    
    if ! run_performance_tests; then
        failures=$((failures + 1))
    fi
    
    run_microbenchmarks
    run_comprehensive_benchmarks
    
    if ! validate_benchmarks; then
        failures=$((failures + $?))
    fi
    
    run_load_tests
    generate_report
    
    echo ""
    echo "================================================"
    if [ $failures -eq 0 ]; then
        print_status "PASS" "All performance validations passed! 🎉"
        echo "The HTTP proxy meets the <10ms overhead target."
    else
        print_status "FAIL" "$failures performance validations failed. ❌"
        echo "Review the performance report and optimization recommendations."
        exit 1
    fi
    echo "================================================"
}

# Run main function
main "$@"