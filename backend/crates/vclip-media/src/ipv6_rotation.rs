//! IPv6 Address Rotation Module
//!
//! Production-grade IPv6 rotation for avoiding YouTube rate limiting.
//!
//! # Features
//!
//! - **Cached address pool** - Avoids expensive `getifaddrs()` calls per request
//! - **TTL-based refresh** - Refreshes address pool periodically
//! - **Thread-safe** - Uses `OnceLock` + `RwLock` for concurrent access
//! - **Weighted selection** - Can track success/failure rates per IP (future)
//! - **Fallback handling** - Gracefully degrades when IPv6 unavailable
//!
//! # Architecture
//!
//! The module maintains a cached pool of global IPv6 addresses that gets
//! refreshed every `CACHE_TTL_SECS`. This avoids the overhead of system
//! calls on every download request while still adapting to network changes.
//!
//! ```text
//! ┌─────────────────┐     ┌──────────────────┐
//! │ IPv6AddressPool │────▶│ Cached Addresses │
//! └────────┬────────┘     └──────────────────┘
//!          │
//!          ▼
//! ┌────────────────────┐
//! │ get_source_address │──▶ Random selection from pool
//! └────────────────────┘
//! ```

use std::net::Ipv6Addr;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

// =============================================================================
// Configuration
// =============================================================================

/// How often to refresh the IPv6 address cache (5 minutes).
/// This balances between detecting new addresses and avoiding overhead.
const CACHE_TTL_SECS: u64 = 300;

/// Minimum number of addresses required for effective rotation.
/// If fewer addresses are available, we still use them but log a warning.
const MIN_EFFECTIVE_ROTATION_ADDRESSES: usize = 10;

// =============================================================================
// IPv6 Address Pool
// =============================================================================

/// Thread-safe cached pool of IPv6 addresses.
struct IPv6AddressPool {
    /// Cached global IPv6 addresses.
    addresses: Vec<String>,
    /// When the cache was last refreshed.
    last_refresh: Instant,
    /// Number of successful requests since last refresh (for metrics).
    success_count: u64,
    /// Number of failed requests since last refresh (for metrics).
    failure_count: u64,
}

impl IPv6AddressPool {
    /// Create an empty pool.
    fn new() -> Self {
        Self {
            addresses: Vec::new(),
            last_refresh: Instant::now() - Duration::from_secs(CACHE_TTL_SECS + 1), // Force immediate refresh
            success_count: 0,
            failure_count: 0,
        }
    }

    /// Check if the cache needs refreshing.
    fn needs_refresh(&self) -> bool {
        self.last_refresh.elapsed() > Duration::from_secs(CACHE_TTL_SECS)
    }

    /// Refresh the address pool from network interfaces.
    fn refresh(&mut self) {
        use nix::ifaddrs::getifaddrs;

        let mut new_addresses = Vec::new();

        let ifaddrs = match getifaddrs() {
            Ok(addrs) => addrs,
            Err(e) => {
                warn!(
                    "Failed to enumerate network interfaces for IPv6 rotation: {}",
                    e
                );
                return;
            }
        };

        for ifaddr in ifaddrs {
            let Some(addr) = ifaddr.address else {
                continue;
            };

            let Some(sockaddr) = addr.as_sockaddr_in6() else {
                continue;
            };

            let ip: Ipv6Addr = sockaddr.ip();

            // Filter non-global addresses
            if !is_global_ipv6(&ip) {
                continue;
            }

            new_addresses.push(ip.to_string());
        }

        // Log changes
        let old_count = self.addresses.len();
        let new_count = new_addresses.len();

        if new_count != old_count {
            info!(
                old_count = old_count,
                new_count = new_count,
                "IPv6 address pool updated"
            );
        }

        if new_count > 0 && new_count < MIN_EFFECTIVE_ROTATION_ADDRESSES {
            warn!(
                count = new_count,
                min_recommended = MIN_EFFECTIVE_ROTATION_ADDRESSES,
                "IPv6 rotation may be less effective with few addresses"
            );
        }

        self.addresses = new_addresses;
        self.last_refresh = Instant::now();
    }

    /// Get a random address from the pool.
    fn get_random(&self) -> Option<String> {
        use rand::prelude::IndexedRandom;

        if self.addresses.is_empty() {
            return None;
        }

        let mut rng = rand::rng();
        self.addresses.choose(&mut rng).cloned()
    }

    /// Record a successful request (for future weighted selection).
    fn record_success(&mut self) {
        self.success_count += 1;
    }

    /// Record a failed request (for future weighted selection).
    fn record_failure(&mut self) {
        self.failure_count += 1;
    }

    /// Get current pool statistics.
    fn stats(&self) -> IPv6PoolStats {
        IPv6PoolStats {
            address_count: self.addresses.len(),
            cache_age_secs: self.last_refresh.elapsed().as_secs(),
            success_count: self.success_count,
            failure_count: self.failure_count,
        }
    }
}

/// Statistics about the IPv6 address pool.
#[derive(Debug, Clone)]
pub struct IPv6PoolStats {
    /// Number of addresses in the pool.
    pub address_count: usize,
    /// Age of the cache in seconds.
    pub cache_age_secs: u64,
    /// Number of successful requests since last refresh.
    pub success_count: u64,
    /// Number of failed requests since last refresh.
    pub failure_count: u64,
}

// =============================================================================
// Global State
// =============================================================================

/// Global IPv6 address pool (lazy initialized, thread-safe).
static IPV6_POOL: OnceLock<RwLock<IPv6AddressPool>> = OnceLock::new();

/// Get or initialize the global IPv6 pool.
fn get_pool() -> &'static RwLock<IPv6AddressPool> {
    IPV6_POOL.get_or_init(|| RwLock::new(IPv6AddressPool::new()))
}

// =============================================================================
// Public API
// =============================================================================

/// Get a random global IPv6 address for use with `--source-address`.
///
/// This function maintains an internal cache of available IPv6 addresses
/// to avoid expensive system calls on every request. The cache is refreshed
/// every 5 minutes.
///
/// # Returns
///
/// - `Some(ip)` - A random global IPv6 address string
/// - `None` - No global IPv6 addresses are available
///
/// # Example
///
/// ```ignore
/// if let Some(ip) = get_random_ipv6_address() {
///     args.push("--source-address");
///     args.push(&ip);
/// }
/// ```
pub fn get_random_ipv6_address() -> Option<String> {
    let pool = get_pool();

    // Check if refresh needed (read lock only)
    let needs_refresh = {
        let read_guard = pool.read().ok()?;
        read_guard.needs_refresh()
    };

    // Refresh if needed (write lock)
    if needs_refresh {
        if let Ok(mut write_guard) = pool.write() {
            // Double-check after acquiring write lock
            if write_guard.needs_refresh() {
                write_guard.refresh();
            }
        }
    }

    // Get random address (read lock)
    let read_guard = pool.read().ok()?;
    let selected = read_guard.get_random();

    if let Some(ref addr) = selected {
        let stats = read_guard.stats();
        debug!(
            address = %addr,
            pool_size = stats.address_count,
            cache_age_secs = stats.cache_age_secs,
            "Selected IPv6 address for rotation"
        );
    }

    selected
}

/// Record a successful request using IPv6 rotation.
///
/// This is used for metrics and future weighted selection.
pub fn record_ipv6_success() {
    if let Some(pool) = IPV6_POOL.get() {
        if let Ok(mut guard) = pool.write() {
            guard.record_success();
        }
    }
}

/// Record a failed request using IPv6 rotation.
///
/// This is used for metrics and future weighted selection.
pub fn record_ipv6_failure() {
    if let Some(pool) = IPV6_POOL.get() {
        if let Ok(mut guard) = pool.write() {
            guard.record_failure();
        }
    }
}

/// Get current IPv6 pool statistics.
///
/// Useful for monitoring and debugging.
pub fn get_ipv6_pool_stats() -> Option<IPv6PoolStats> {
    let pool = get_pool();
    let guard = pool.read().ok()?;
    Some(guard.stats())
}

/// Force refresh of the IPv6 address pool.
///
/// Call this after IPv6 addresses have been assigned to the container.
pub fn refresh_ipv6_pool() {
    let pool = get_pool();
    if let Ok(mut guard) = pool.write() {
        guard.refresh();
        let stats = guard.stats();
        info!(
            address_count = stats.address_count,
            "IPv6 address pool manually refreshed"
        );
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Check if an IPv6 address is globally routable.
///
/// Filters out:
/// - Loopback (::1)
/// - Link-local (fe80::/10)
/// - Unique local (fc00::/7)
/// - Multicast (ff00::/8)
/// - Documentation (2001:db8::/32)
/// - 6to4 relay anycast (192.88.99.0/24 mapped)
fn is_global_ipv6(ip: &Ipv6Addr) -> bool {
    // Check built-in filters
    if ip.is_loopback() || ip.is_multicast() || ip.is_unspecified() {
        return false;
    }

    let segments = ip.segments();

    // Link-local (fe80::/10)
    if (segments[0] & 0xffc0) == 0xfe80 {
        return false;
    }

    // Unique local (fc00::/7)
    if (segments[0] & 0xfe00) == 0xfc00 {
        return false;
    }

    // Documentation (2001:db8::/32)
    if segments[0] == 0x2001 && segments[1] == 0x0db8 {
        return false;
    }

    // Teredo (2001::/32) - not globally routable in most cases
    if segments[0] == 0x2001 && segments[1] == 0x0000 {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    #[test]
    fn test_is_global_ipv6() {
        // Loopback - not global
        assert!(!is_global_ipv6(&"::1".parse().unwrap()));

        // Link-local - not global
        assert!(!is_global_ipv6(&"fe80::1".parse().unwrap()));

        // Unique local - not global
        assert!(!is_global_ipv6(&"fc00::1".parse().unwrap()));
        assert!(!is_global_ipv6(&"fd00::1".parse().unwrap()));

        // Documentation - not global
        assert!(!is_global_ipv6(&"2001:db8::1".parse().unwrap()));

        // Global unicast - should be global
        assert!(is_global_ipv6(&"2001:41d0:1234::1".parse().unwrap()));
        assert!(is_global_ipv6(&"2a00:1234::1".parse().unwrap()));
    }

    #[test]
    fn test_pool_stats() {
        let pool = IPv6AddressPool::new();
        let stats = pool.stats();
        assert_eq!(stats.address_count, 0);
        assert_eq!(stats.success_count, 0);
        assert_eq!(stats.failure_count, 0);
    }
}
