// Memory pooling and streaming body handling for ultra-fast processing
use std::collections::HashMap;
use std::sync::Arc;
use bytes::{Bytes, BytesMut};
use axum::body::Body;
use http_body_util::BodyExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::performance::{BODY_POOL_COUNT, Metrics, time_section};

/// Tiered buffer pool for different sizes
pub struct BufferPool {
    small_buffers: Vec<BytesMut>,     // 256B - 1KB
    medium_buffers: Vec<BytesMut>,    // 1KB - 4KB  
    large_buffers: Vec<BytesMut>,     // 4KB - 16KB
    huge_buffers: Vec<BytesMut>,      // 16KB - 64KB
}

impl BufferPool {
    pub fn new() -> Self {
        Self {
            small_buffers: Vec::with_capacity(100),
            medium_buffers: Vec::with_capacity(50),
            large_buffers: Vec::with_capacity(25),
            huge_buffers: Vec::with_capacity(10),
        }
    }

    /// Get buffer of appropriate size
    pub fn get_buffer(&mut self, size: usize) -> BytesMut {
        match size {
            0..=1024 => self.small_buffers.pop()
                .unwrap_or_else(|| BytesMut::with_capacity(1024)),
            1025..=4096 => self.medium_buffers.pop()
                .unwrap_or_else(|| BytesMut::with_capacity(4096)),
            4097..=16384 => self.large_buffers.pop()
                .unwrap_or_else(|| BytesMut::with_capacity(16384)),
            _ => self.huge_buffers.pop()
                .unwrap_or_else(|| BytesMut::with_capacity(size)),
        }
    }

    /// Return buffer to pool
    pub fn return_buffer(&mut self, mut buffer: BytesMut) {
        buffer.clear();
        
        match buffer.capacity() {
            0..=1024 if self.small_buffers.len() < 100 => {
                self.small_buffers.push(buffer);
            },
            1025..=4096 if self.medium_buffers.len() < 50 => {
                self.medium_buffers.push(buffer);
            },
            4097..=16384 if self.large_buffers.len() < 25 => {
                self.large_buffers.push(buffer);
            },
            _ if self.huge_buffers.len() < 10 => {
                self.huge_buffers.push(buffer);
            },
            _ => {} // Pool is full, let it drop
        }
    }
}

/// Thread-local buffer pool
thread_local! {
    static BUFFER_POOL: std::cell::RefCell<BufferPool> = 
        std::cell::RefCell::new(BufferPool::new());
}

/// String pool for reducing allocations
pub struct StringPool {
    strings: HashMap<String, &'static str>,
    next_id: usize,
}

impl StringPool {
    pub fn new() -> Self {
        Self {
            strings: HashMap::new(),
            next_id: 0,
        }
    }

    /// Get interned string or create new one
    pub fn intern(&mut self, s: String) -> &'static str {
        if let Some(interned) = self.strings.get(&s) {
            return *interned;
        }
        
        // Leak string to get 'static lifetime
        let interned = Box::leak(s.into_boxed_str());
        self.strings.insert(s, interned);
        interned
    }
}

/// Thread-local string pool
thread_local! {
    static STRING_POOL: std::cell::RefCell<StringPool> = 
        std::cell::RefCell::new(StringPool::new());
}

/// Streaming body processor for memory efficiency
pub struct BodyProcessor;

impl BodyProcessor {
    /// Process body with streaming and minimal allocation
    pub async fn process_body_streaming(
        body: Body,
        metrics: &mut Metrics,
        max_size: usize
    ) -> Result<(Bytes, Option<String>), Box<dyn std::error::Error + Send + Sync>> {
        time_section!(start, metrics.body_processing_time, {
            BUFFER_POOL.with(|pool| {
                let mut pool = pool.borrow_mut();
                let mut buffer = pool.get_buffer(max_size.min(65536)); // Max 64KB buffer
                
                let mut total_size = 0;
                let mut reader = body.into_data_stream();
                
                use futures_util::StreamExt;
                while let Some(chunk) = reader.next().await {
                    let chunk = chunk?;
                    total_size += chunk.len();
                    
                    if total_size > max_size {
                        return Err("Body too large".into());
                    }
                    
                    if buffer.len() + chunk.len() <= buffer.capacity() {
                        buffer.extend_from_slice(&chunk);
                    } else {
                        // Buffer full, process current and continue
                        let _current = buffer.split().freeze();
                        buffer.extend_from_slice(&chunk);
                    }
                }
                
                let final_bytes = buffer.split().freeze();
                
                // Only convert to string if needed for logging
                let string_content = if total_size <= 8192 { // Only for smaller bodies
                    Some(String::from_utf8_lossy(&final_bytes).to_string())
                } else {
                    None
                };
                
                pool.return_buffer(buffer);
                
                Ok((final_bytes, string_content))
            })
        })
    }

    /// Fast body processing for small bodies
    pub async fn process_body_fast(
        body: Body,
        metrics: &mut Metrics
    ) -> Result<(Bytes, String), Box<dyn std::error::Error + Send + Sync>> {
        time_section!(start, metrics.body_processing_time, {
            let bytes = body.collect().await?.to_bytes();
            let string_content = String::from_utf8_lossy(&bytes).to_string();
            
            Ok((bytes, string_content))
        })
    }

    /// Ultra-fast body processing for common patterns
    pub async fn process_body_smart(
        body: Body,
        metrics: &mut Metrics,
        is_logging_enabled: bool
    ) -> Result<(Bytes, Option<String>), Box<dyn std::error::Error + Send + Sync>> {
        // Estimate body size quickly
        let size_hint = body.size_hint();
        let estimated_size = size_hint.upper().unwrap_or(0);
        
        if estimated_size < 8192 {
            // Small body - use fast path
            let (bytes, string) = Self::process_body_fast(body, metrics).await?;
            Ok((bytes, Some(string)))
        } else if is_logging_enabled {
            // Large body with logging - use streaming with string conversion
            Self::process_body_streaming(body, metrics, 1048576).await // Max 1MB
        } else {
            // Large body without logging - use streaming without string conversion
            Self::process_body_streaming(body, metrics, 1048576).await
        }
    }
}

/// Memory usage tracker for performance monitoring
pub struct MemoryTracker {
    pub allocations: usize,
    pub deallocations: usize,
    pub current_usage: usize,
    pub peak_usage: usize,
}

impl MemoryTracker {
    pub fn new() -> Self {
        Self {
            allocations: 0,
            deallocations: 0,
            current_usage: 0,
            peak_usage: 0,
        }
    }

    pub fn allocate(&mut self, size: usize) {
        self.allocations += 1;
        self.current_usage += size;
        if self.current_usage > self.peak_usage {
            self.peak_usage = self.current_usage;
        }
    }

    pub fn deallocate(&mut self, size: usize) {
        self.deallocations += 1;
        self.current_usage = self.current_usage.saturating_sub(size);
    }
}

/// Thread-local memory tracker
thread_local! {
    static MEMORY_TRACKER: std::cell::RefCell<MemoryTracker> = 
        std::cell::RefCell::new(MemoryTracker::new());
}

/// Get memory usage statistics
pub fn get_memory_stats() -> MemoryStats {
    MEMORY_TRACKER.with(|tracker| {
        let t = tracker.borrow();
        MemoryStats {
            allocations: t.allocations,
            deallocations: t.deallocations,
            current_usage: t.current_usage,
            peak_usage: t.peak_usage,
        }
    })
}

#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub allocations: usize,
    pub deallocations: usize,
    pub current_usage: usize,
    pub peak_usage: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_buffer_pool() {
        let mut pool = BufferPool::new();
        
        let buffer1 = pool.get_buffer(512);
        let capacity1 = buffer1.capacity();
        
        pool.return_buffer(buffer1);
        let buffer2 = pool.get_buffer(512);
        
        // Should reuse the same buffer
        assert_eq!(buffer2.capacity(), capacity1);
    }

    #[tokio::test]
    async fn test_body_processing_fast() {
        let body = Body::from("small test body");
        let mut metrics = Metrics::new();
        
        let (bytes, string) = BodyProcessor::process_body_fast(body, &mut metrics).await.unwrap();
        
        assert_eq!(string, "small test body");
        assert_eq!(bytes.len(), 15);
        assert!(metrics.body_processing_time.as_micros() < 100);
    }

    #[tokio::test]
    async fn test_body_processing_smart() {
        // Small body - should use fast path
        let small_body = Body::from("small");
        let mut metrics = Metrics::new();
        
        let (bytes, string) = BodyProcessor::process_body_smart(small_body, &mut metrics, true).await.unwrap();
        
        assert!(string.is_some());
        assert_eq!(string.unwrap(), "small");
        
        // Large body - should use streaming path
        let large_body = Body::from(vec![0u8; 10000]);
        let mut metrics2 = Metrics::new();
        
        let (bytes2, string2) = BodyProcessor::process_body_smart(large_body, &mut metrics2, false).await.unwrap();
        
        assert!(string2.is_none()); // No string for large body without logging
        assert_eq!(bytes2.len(), 10000);
    }

    #[test]
    fn test_memory_tracking() {
        MEMORY_TRACKER.with(|tracker| {
            let mut t = tracker.borrow_mut();
            t.allocate(1024);
            t.allocate(2048);
            t.deallocate(1024);
            
            let stats = get_memory_stats();
            assert_eq!(stats.allocations, 2);
            assert_eq!(stats.deallocations, 1);
            assert_eq!(stats.current_usage, 2048);
            assert_eq!(stats.peak_usage, 3072);
        });
    }
}

impl BytesPool {
    pub fn new() -> Self {
        Self::with_config([256, 512, 1024, 4096, 8192, 16384], 100)
    }

    pub fn with_config(sizes: impl IntoIterator<Item = usize>, max_pool_size: usize) -> Self {
        let pools: Vec<_> = sizes
            .into_iter()
            .map(|size| Mutex::new(VecDeque::with_capacity(max_pool_size)))
            .collect();

        Self {
            pools,
            max_pool_size,
        }
    }

    // Get buffer with at least the specified capacity
    pub fn get_buffer(&self, capacity: usize) -> BytesMut {
        let pool_index = self.find_pool_index(capacity);

        if let Some(pool) = self.pools.get(pool_index) {
            let mut deque = pool.lock();
            if let Some(mut buf) = deque.pop_front() {
                buf.clear();
                buf.reserve(capacity.saturating_sub(buf.capacity()));
                return buf;
            }
        }

        // No suitable buffer in pool, allocate new
        let size = self.pool_size(pool_index);
        BytesMut::with_capacity(size.max(capacity))
    }

    // Return buffer to pool
    pub fn return_buffer(&self, mut buffer: BytesMut) {
        let capacity = buffer.capacity();
        let pool_index = self.find_pool_index(capacity);

        if let Some(pool) = self.pools.get(pool_index) {
            let mut deque = pool.lock();
            if deque.len() < self.max_pool_size {
                buffer.clear();
                deque.push_back(buffer);
            }
        }
    }

    fn find_pool_index(&self, capacity: usize) -> usize {
        // Find smallest pool that can accommodate the capacity
        for (i, &size) in [256, 512, 1024, 4096, 8192, 16384].iter().enumerate() {
            if capacity <= size {
                return i;
            }
        }
        self.pools.len().saturating_sub(1)
    }

    fn pool_size(&self, index: usize) -> usize {
        match index {
            0 => 256,
            1 => 512,
            2 => 1024,
            3 => 4096,
            4 => 8192,
            5 => 16384,
            _ => 16384,
        }
    }

    pub fn stats(&self) -> PoolStats {
        let mut stats = PoolStats::default();

        for (i, pool) in self.pools.iter().enumerate() {
            let deque = pool.lock();
            stats.pool_sizes.push(self.pool_size(i));
            stats.pool_counts.push(deque.len());
            stats.total_buffers += deque.len();
        }

        stats
    }
}

#[derive(Debug, Default)]
pub struct PoolStats {
    pub pool_sizes: Vec<usize>,
    pub pool_counts: Vec<usize>,
    pub total_buffers: usize,
}

// Global bytes pool instance
static BYTES_POOL: std::sync::LazyLock<BytesPool> = std::sync::LazyLock::new(BytesPool::new);

pub fn get_bytes_buffer(capacity: usize) -> BytesMut {
    BYTES_POOL.get_buffer(capacity)
}

pub fn return_bytes_buffer(buffer: BytesMut) {
    BYTES_POOL.return_buffer(buffer);
}

// String pool for common strings
pub struct StringPool {
    pool: Mutex<Vec<String>>,
    max_pool_size: usize,
}

impl StringPool {
    pub fn new(max_pool_size: usize) -> Self {
        Self {
            pool: Mutex::new(Vec::with_capacity(max_pool_size)),
            max_pool_size,
        }
    }

    pub fn get_string(&self) -> String {
        let mut pool = self.pool.lock();
        pool.pop().unwrap_or_else(|| String::with_capacity(256))
    }

    pub fn return_string(&self, mut string: String) {
        let mut pool = self.pool.lock();
        if pool.len() < self.max_pool_size {
            string.clear();
            pool.push(string);
        }
    }
}

// Thread-local string pool for ultra-fast access
thread_local! {
    static LOCAL_STRING_POOL: std::cell::RefCell<Vec<String>> =
        std::cell::RefCell::new(Vec::with_capacity(16));
}

pub fn get_local_string() -> String {
    LOCAL_STRING_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        pool.pop().unwrap_or_else(|| String::with_capacity(256))
    })
}

pub fn return_local_string(mut string: String) {
    LOCAL_STRING_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < 16 {
            string.clear();
            pool.push(string);
        }
    })
}

// Optimized body handling with streaming
pub struct StreamingBody {
    bytes: Bytes,
}

impl StreamingBody {
    pub fn from_bytes(bytes: Bytes) -> Self {
        Self { bytes }
    }

    pub fn into_bytes(self) -> Bytes {
        self.bytes
    }

    // Convert to string only when needed
    pub fn to_string_lossy(&self) -> String {
        let mut string = get_local_string();
        string.push_str(&String::from_utf8_lossy(&self.bytes));
        string
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_pool() {
        let pool = BytesPool::new();

        // Get and return buffers
        let buf1 = pool.get_buffer(1024);
        let buf2 = pool.get_buffer(512);

        pool.return_buffer(buf1);
        pool.return_buffer(buf2);

        // Should reuse buffers
        let buf3 = pool.get_buffer(800);
        assert!(buf3.capacity() >= 800);
    }

    #[test]
    fn test_string_pool() {
        let pool = StringPool::new(10);

        let mut s1 = pool.get_string();
        s1.push_str("test content");

        pool.return_string(s1);

        let s2 = pool.get_string();
        assert!(s2.capacity() >= 256);
        assert!(s2.is_empty()); // Should be cleared
    }

    #[test]
    fn test_streaming_body() {
        let data = b"test body content";
        let body = StreamingBody::from_bytes(Bytes::from(&data[..]));

        assert_eq!(body.len(), data.len());
        assert_eq!(body.as_slice(), data);

        let string = body.to_string_lossy();
        assert_eq!(string, "test body content");
    }

    #[test]
    fn test_local_string_pool() {
        let s1 = get_local_string();
        return_local_string(s1);

        let s2 = get_local_string();
        assert!(s2.capacity() >= 256);
    }
}
