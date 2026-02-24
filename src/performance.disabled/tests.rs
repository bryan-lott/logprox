// Performance module tests
use std::time::{Duration, Instant};

/// Performance module tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_constants() {
        // Verify performance targets are reasonable
        assert!(crate::performance::TARGET_MAX_PROCESSING_TIME_US > 0);
        assert!(crate::performance::TARGET_MAX_PROCESSING_TIME_US <= 1000); // Should be ≤ 1ms
    }

    #[test]
    fn test_duration_operations() {
        let duration = Duration::from_micros(500);
        assert_eq!(duration.as_micros(), 500);
        assert!(duration < Duration::from_millis(1));
    }
}
