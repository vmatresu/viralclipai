//! Performance module for video processing.
//!
//! Provides connection pooling, resource management, caching, and performance
//! optimizations for high-throughput video processing.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;

use crate::error::{MediaError, MediaResult};

/// Connection pool for managing FFmpeg processes.
/// Prevents resource exhaustion and provides efficient reuse.
#[derive(Clone)]
pub struct FFmpegPool {
    pool: Arc<Mutex<VecDeque<FFmpegConnection>>>,
    max_connections: usize,
    max_idle_time: Duration,
    semaphore: Arc<Semaphore>,
}

impl FFmpegPool {
    /// Create a new FFmpeg connection pool.
    pub fn new(max_connections: usize) -> Self {
        Self {
            pool: Arc::new(Mutex::new(VecDeque::new())),
            max_connections,
            max_idle_time: Duration::from_secs(300), // 5 minutes
            semaphore: Arc::new(Semaphore::new(max_connections)),
        }
    }

    /// Acquire a connection from the pool.
    pub async fn acquire(&self) -> MediaResult<FFmpegConnection> {
        // Wait for available permit
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| MediaError::ResourceLimit("Semaphore closed".to_string()))?;

        // Try to get an existing connection
        if let Some(conn) = self.pool.lock().unwrap().pop_front() {
            if conn.is_valid() {
                return Ok(conn);
            }
        }

        // Create a new connection
        let conn = FFmpegConnection::new();
        Ok(conn)
    }

    /// Return a connection to the pool.
    pub fn release(&self, mut conn: FFmpegConnection) {
        conn.last_used = Instant::now();

        let mut pool = self.pool.lock().unwrap();
        if pool.len() < self.max_connections {
            pool.push_back(conn);
        }
        // If pool is full, connection is dropped
    }

    /// Clean up idle connections.
    pub fn cleanup_idle(&self) {
        let mut pool = self.pool.lock().unwrap();
        let now = Instant::now();

        pool.retain(|conn| now.duration_since(conn.last_used) < self.max_idle_time);
    }

    /// Get pool statistics.
    pub fn stats(&self) -> PoolStats {
        let pool = self.pool.lock().unwrap();
        PoolStats {
            active_connections: self.max_connections - self.semaphore.available_permits(),
            idle_connections: pool.len(),
            max_connections: self.max_connections,
        }
    }
}

impl Default for FFmpegPool {
    fn default() -> Self {
        Self::new(10) // Default 10 concurrent FFmpeg processes
    }
}

/// Individual FFmpeg connection wrapper.
pub struct FFmpegConnection {
    id: String,
    #[allow(dead_code)]
    created_at: Instant,
    last_used: Instant,
    process_count: u64,
}

impl FFmpegConnection {
    fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: Instant::now(),
            last_used: Instant::now(),
            process_count: 0,
        }
    }

    fn is_valid(&self) -> bool {
        // Check if connection is still usable
        // In a real implementation, this might ping FFmpeg or check system resources
        self.process_count < 1000 // Arbitrary limit
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn mark_used(&mut self) {
        self.last_used = Instant::now();
        self.process_count += 1;
    }
}

/// Pool statistics for monitoring.
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub active_connections: usize,
    pub idle_connections: usize,
    pub max_connections: usize,
}

/// Resource manager for coordinating access to shared resources.
#[derive(Clone)]
pub struct ResourceManager {
    ffmpeg_pool: FFmpegPool,
    temp_dir_manager: TempDirectoryManager,
    cache: ProcessingCache,
}

impl ResourceManager {
    /// Create a new resource manager.
    pub fn new() -> Self {
        Self {
            ffmpeg_pool: FFmpegPool::default(),
            temp_dir_manager: TempDirectoryManager::new(),
            cache: ProcessingCache::new(),
        }
    }

    /// Get access to FFmpeg pool.
    pub fn ffmpeg_pool(&self) -> &FFmpegPool {
        &self.ffmpeg_pool
    }

    /// Get access to temp directory manager.
    pub fn temp_manager(&self) -> &TempDirectoryManager {
        &self.temp_dir_manager
    }

    /// Get access to processing cache.
    pub fn cache(&self) -> &ProcessingCache {
        &self.cache
    }

    /// Perform periodic cleanup of resources.
    pub async fn cleanup(&self) {
        self.ffmpeg_pool.cleanup_idle();
        self.temp_dir_manager.cleanup_old_dirs().await;
        self.cache.cleanup_expired().await;
    }
}

impl Default for ResourceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Manager for temporary directories with automatic cleanup.
#[derive(Clone)]
pub struct TempDirectoryManager {
    base_dir: std::path::PathBuf,
    dirs: Arc<RwLock<HashMap<String, TempDirInfo>>>,
    max_age: Duration,
}

#[derive(Clone)]
struct TempDirInfo {
    path: std::path::PathBuf,
    created_at: Instant,
    #[allow(dead_code)]
    request_id: String,
}

impl TempDirectoryManager {
    /// Create a new temp directory manager.
    pub fn new() -> Self {
        Self {
            base_dir: std::env::temp_dir().join("vclip_processing"),
            dirs: Arc::new(RwLock::new(HashMap::new())),
            max_age: Duration::from_secs(3600),
        }
    }

    /// Create a new temporary directory for a request.
    pub async fn create_temp_dir(&self, request_id: &str) -> MediaResult<std::path::PathBuf> {
        let dir_name = format!("{}_{}", request_id, uuid::Uuid::new_v4().simple());
        let dir_path = self.base_dir.join(dir_name);

        tokio::fs::create_dir_all(&dir_path)
            .await
            .map_err(MediaError::Io)?;

        let info = TempDirInfo {
            path: dir_path.clone(),
            created_at: Instant::now(),
            request_id: request_id.to_string(),
        };

        self.dirs
            .write()
            .unwrap()
            .insert(request_id.to_string(), info);

        Ok(dir_path)
    }

    /// Clean up temporary directory for a request.
    pub async fn cleanup_request(&self, request_id: &str) {
        if let Some(info) = self.dirs.write().unwrap().remove(request_id) {
            let _ = tokio::fs::remove_dir_all(&info.path).await;
        }
    }

    /// Clean up old temporary directories.
    pub async fn cleanup_old_dirs(&self) {
        let now = Instant::now();
        let mut dirs_to_remove = Vec::new();

        // Find old directories
        for (request_id, info) in self.dirs.read().unwrap().iter() {
            if now.duration_since(info.created_at) > self.max_age {
                dirs_to_remove.push(request_id.clone());
            }
        }

        // Remove them
        for request_id in dirs_to_remove {
            if let Some(info) = self.dirs.write().unwrap().remove(&request_id) {
                let _ = tokio::fs::remove_dir_all(&info.path).await;
            }
        }
    }
}

/// Processing cache for expensive operations.
#[derive(Clone)]
pub struct ProcessingCache {
    video_info: Arc<RwLock<HashMap<String, CachedVideoInfo>>>,
    max_age: Duration,
}

#[derive(Clone)]
struct CachedVideoInfo {
    info: crate::probe::VideoInfo,
    cached_at: Instant,
}

impl ProcessingCache {
    /// Create a new processing cache.
    pub fn new() -> Self {
        Self {
            video_info: Arc::new(RwLock::new(HashMap::new())),
            max_age: Duration::from_secs(1800),
        }
    }

    /// Get cached video info or compute it.
    pub async fn get_video_info(
        &self,
        path: &std::path::Path,
    ) -> MediaResult<crate::probe::VideoInfo> {
        let key = path.to_string_lossy().to_string();
        let now = Instant::now();

        // Check cache first
        {
            let cache = self.video_info.read().unwrap();
            if let Some(cached) = cache.get(&key) {
                if now.duration_since(cached.cached_at) < self.max_age {
                    return Ok(cached.info.clone());
                }
            }
        }

        // Compute and cache
        let info = crate::probe::probe_video(path).await?;
        let cached = CachedVideoInfo {
            info: info.clone(),
            cached_at: now,
        };

        self.video_info.write().unwrap().insert(key, cached);
        Ok(info)
    }

    /// Clean up expired cache entries.
    pub async fn cleanup_expired(&self) {
        let now = Instant::now();
        self.video_info
            .write()
            .unwrap()
            .retain(|_, info| now.duration_since(info.cached_at) < self.max_age);
    }
}

/// Performance monitoring utilities.
pub mod monitoring {
    use super::*;

    /// Performance metrics for operations.
    #[derive(Debug, Clone)]
    pub struct PerformanceMetrics {
        pub operation: String,
        pub start_time: Instant,
        pub cpu_time: Option<Duration>,
        pub memory_peak_mb: Option<f64>,
        pub io_bytes: Option<u64>,
    }

    impl PerformanceMetrics {
        /// Start measuring performance.
        pub fn start(operation: &str) -> Self {
            Self {
                operation: operation.to_string(),
                start_time: Instant::now(),
                cpu_time: None,
                memory_peak_mb: None,
                io_bytes: None,
            }
        }

        /// Complete measurement and get duration.
        pub fn complete(self) -> Duration {
            self.start_time.elapsed()
        }
    }

    /// Circuit breaker for external service calls.
    #[derive(Clone)]
    pub struct CircuitBreaker {
        state: Arc<RwLock<CircuitState>>,
        #[allow(dead_code)]
        failure_threshold: u32,
        recovery_timeout: Duration,
        success_threshold: u32,
    }

    #[derive(Clone)]
    enum CircuitState {
        Closed,
        Open { opened_at: Instant },
        HalfOpen { success_count: u32 },
    }

    impl CircuitBreaker {
        /// Create a new circuit breaker.
        pub fn new(
            failure_threshold: u32,
            recovery_timeout: Duration,
            success_threshold: u32,
        ) -> Self {
            Self {
                state: Arc::new(RwLock::new(CircuitState::Closed)),
                failure_threshold,
                recovery_timeout,
                success_threshold,
            }
        }

        /// Check if operation is allowed.
        pub fn allow(&self) -> bool {
            let mut state = self.state.write().unwrap();
            match *state {
                CircuitState::Closed => true,
                CircuitState::Open { opened_at } => {
                    if Instant::now().duration_since(opened_at) > self.recovery_timeout {
                        *state = CircuitState::HalfOpen { success_count: 0 };
                        true
                    } else {
                        false
                    }
                }
                CircuitState::HalfOpen { .. } => true,
            }
        }

        /// Record a successful operation.
        pub fn success(&self) {
            let mut state = self.state.write().unwrap();
            match *state {
                CircuitState::HalfOpen { success_count } => {
                    let new_count = success_count + 1;
                    if new_count >= self.success_threshold {
                        *state = CircuitState::Closed;
                    } else {
                        *state = CircuitState::HalfOpen {
                            success_count: new_count,
                        };
                    }
                }
                _ => {} // No change for other states
            }
        }

        /// Record a failed operation.
        pub fn failure(&self) {
            let mut state = self.state.write().unwrap();
            match *state {
                CircuitState::Closed | CircuitState::HalfOpen { .. } => {
                    *state = CircuitState::Open {
                        opened_at: Instant::now(),
                    };
                }
                CircuitState::Open { .. } => {} // Already open
            }
        }
    }
}
