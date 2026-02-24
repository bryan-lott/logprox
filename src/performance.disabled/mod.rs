// Ultra-high performance HTTP proxy optimization module
// Target: <500μs non-network overhead (90%+ reduction from current 6-13ms)

pub mod cache;
pub mod zero_copy;
pub mod pool;
pub mod lockfree;
pub mod benchmark;
pub mod tests;

use std::time::{Duration, Instant};

/// Performance metrics collection
#[derive(Debug, Clone)]
pub struct Metrics {
    pub regex_cache_time: Duration,
    pub header_processing_time: Duration,
    pub body_processing_time: Duration,
    pub config_lookup_time: Duration,
    pub total_overhead: Duration,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            regex_cache_time: Duration::ZERO,
            header_processing_time: Duration::ZERO,
            body_processing_time: Duration::ZERO,
            config_lookup_time: Duration::ZERO,
            total_overhead: Duration::ZERO,
        }
    }

    pub fn add(&mut self, other: &Metrics) {
        self.regex_cache_time += other.regex_cache_time;
        self.header_processing_time += other.header_processing_time;
        self.body_processing_time += other.body_processing_time;
        self.config_lookup_time += other.config_lookup_time;
        self.total_overhead += other.total_overhead;
    }

    pub fn total(&self) -> Duration {
        self.regex_cache_time + self.header_processing_time + self.body_processing_time + self.config_lookup_time
    }

    pub fn as_micros(&self) -> u64 {
        self.total().as_micros()
    }

    pub fn is_under_target(&self, target_micros: u64) -> bool {
        self.as_micros() < target_micros
    }
}

/// Global performance settings
pub const TARGET_OVERHEAD_MICROS: u64 = 500; // Ultra-aggressive target
pub const REGEX_CACHE_SIZE: usize = 10_000;
pub const HEADER_POOL_SIZE: usize = 1_000;
pub const BODY_POOL_COUNT: usize = 100;

/// Performance validation macros for measuring overhead
#[macro_export]
macro_rules! time_section {
    ($start:ident, $duration:ident, $block:block) => {
        let $start = Instant::now();
        $block
        $duration += $start.elapsed();
    };
}

#[macro_export]
macro_rules! time_opt {
    ($name:expr, $block:block) => {
        let start = Instant::now();
        let result = $block;
        let elapsed = start.elapsed();
        if elapsed.as_micros() > 100 {
            tracing::warn!("Slow operation {}: {}μs", $name, elapsed.as_micros());
        }
        result
    };
}

#[cfg(test)]
mod tests;