// Ultra-fast thread-local regex caching for sub-millisecond performance
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::performance::{time_section, Metrics, REGEX_CACHE_SIZE};

/// Thread-local regex cache for ultra-fast access
thread_local! {
    static THREAD_REGEX_CACHE: std::cell::RefCell<HashMap<String, Arc<Regex>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Global shared regex cache with LRU eviction
static GLOBAL_REGEX_CACHE: Lazy<RwLock<lru::LruCache<String, Arc<Regex>>>> =
    Lazy::new(|| RwLock::new(lru::LruCache::new(REGEX_CACHE_SIZE)));

/// High-performance regex cache with thread-local + shared strategy
pub struct RegexCache;

impl RegexCache {
    /// Get or compile regex with ultra-fast path
    #[inline(always)]
    pub fn get_or_compile(pattern: &str, metrics: &mut Metrics) -> Arc<Regex> {
        time_section!(start, metrics.regex_cache_time, {
            // Try thread-local first (fastest path)
            THREAD_REGEX_CACHE.with(|cache| {
                if let Some(regex) = cache.borrow().get(pattern) {
                    return Arc::clone(regex);
                }

                // Try global cache
                {
                    let global_cache = GLOBAL_REGEX_CACHE.read().unwrap();
                    if let Some(regex) = global_cache.peek(pattern) {
                        // Add to thread-local for next time
                        cache
                            .borrow_mut()
                            .insert(pattern.to_string(), Arc::clone(regex));
                        return Arc::clone(regex);
                    }
                }

                // Compile and cache
                let regex = Arc::new(Regex::new(pattern).unwrap_or_else(|_| {
                    // Fallback to a regex that never matches
                    Regex::new(r".^ NEVER_MATCH $").unwrap()
                }));

                // Add to both caches
                cache
                    .borrow_mut()
                    .insert(pattern.to_string(), Arc::clone(&regex));

                {
                    let mut global_cache = GLOBAL_REGEX_CACHE.write().unwrap();
                    global_cache.put(pattern.to_string(), Arc::clone(&regex));
                }

                regex
            })
        })
    }

    /// Clear caches (for testing)
    pub fn clear() {
        THREAD_REGEX_CACHE.with(|cache| cache.borrow_mut().clear());
        let mut global_cache = GLOBAL_REGEX_CACHE.write().unwrap();
        global_cache.clear();
    }

    /// Get cache statistics
    pub fn stats() -> CacheStats {
        let thread_count = THREAD_REGEX_CACHE.with(|cache| cache.borrow().len());
        let global_count = GLOBAL_REGEX_CACHE.read().unwrap().len();

        CacheStats {
            thread_cache_size: thread_count,
            global_cache_size: global_count,
            total_cached: thread_count + global_count,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub thread_cache_size: usize,
    pub global_cache_size: usize,
    pub total_cached: usize,
}

/// Pre-compiled common regex patterns for instant access
pub static COMMON_REGEXES: Lazy<HashMap<&'static str, Arc<Regex>>> = Lazy::new(|| {
    let mut map = HashMap::new();

    // Pre-compile common patterns
    let patterns = [
        (r"^GET ", r"GET "),
        (r"^POST ", r"POST "),
        (r"^PUT ", r"PUT "),
        (r"^DELETE ", r"DELETE "),
        (r"^/health$", r"^/health$"),
        (r"^/api/", r"^/api/"),
        (r"application/json", r"application/json"),
        (r"text/html", r"text/html"),
        (r"user-agent", r"user-agent"),
        (r"content-type", r"content-type"),
        (r".*", r".*"), // Catch-all pattern
    ];

    for (key, pattern) in patterns {
        map.insert(key, Arc::new(Regex::new(pattern).unwrap()));
    }

    map
});

/// Ultra-fast pattern matching for common patterns
#[inline(always)]
pub fn match_common_pattern(pattern: &str, text: &str) -> Option<bool> {
    COMMON_REGEXES
        .get(pattern)
        .map(|regex| regex.is_match(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_regex_cache_performance() {
        let pattern = r"user-id: \d+";
        let mut metrics = Metrics::new();

        // First call - should be slower
        let start = std::time::Instant::now();
        let _regex1 = RegexCache::get_or_compile(pattern, &mut metrics);
        let first_time = start.elapsed();

        // Second call - should be much faster
        let start = std::time::Instant::now();
        let _regex2 = RegexCache::get_or_compile(pattern, &mut metrics);
        let second_time = start.elapsed();

        assert!(second_time < first_time);
        assert!(metrics.regex_cache_time.as_micros() < 1000); // Should be under 1ms total
    }

    #[test]
    fn test_common_pattern_matching() {
        assert_eq!(match_common_pattern(r".*", "anything"), Some(true));
        assert_eq!(match_common_pattern(r"^/health$", "/health"), Some(true));
        assert_eq!(match_common_pattern(r"^/api/", "/api/users"), Some(true));
        assert_eq!(match_common_pattern(r"^GET ", "GET /api/users"), Some(true));
        assert_eq!(match_common_pattern(r"^POST ", "GET /api/users"), None); // Doesn't match
    }

    #[test]
    fn test_cache_stats() {
        RegexCache::clear();
        let stats = RegexCache::stats();
        assert_eq!(stats.total_cached, 0);

        let _regex = RegexCache::get_or_compile("test.*", &mut Metrics::new());
        let stats_after = RegexCache::stats();
        assert!(stats_after.total_cached > 0);
    }
}
