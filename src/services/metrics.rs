use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Performance metrics for a single operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub operation: String,
    pub duration_ms: f64,
    pub cache_hit: bool,
    pub db_queries: u32,
    pub timestamp: DateTime<Utc>,
}

impl PerformanceMetrics {
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            duration_ms: 0.0,
            cache_hit: false,
            db_queries: 0,
            timestamp: Utc::now(),
        }
    }

    pub fn with_duration(mut self, duration_ms: f64) -> Self {
        self.duration_ms = duration_ms;
        self
    }

    pub fn with_cache_hit(mut self, cache_hit: bool) -> Self {
        self.cache_hit = cache_hit;
        self
    }

    pub fn with_db_queries(mut self, db_queries: u32) -> Self {
        self.db_queries = db_queries;
        self
    }
}

/// Aggregated statistics from collected metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsStats {
    pub total_operations: usize,
    pub avg_duration_ms: f64,
    pub p95_duration_ms: f64,
    pub p99_duration_ms: f64,
    pub cache_hit_rate: f64,
    pub total_db_queries: u64,
}

impl Default for MetricsStats {
    fn default() -> Self {
        Self {
            total_operations: 0,
            avg_duration_ms: 0.0,
            p95_duration_ms: 0.0,
            p99_duration_ms: 0.0,
            cache_hit_rate: 0.0,
            total_db_queries: 0,
        }
    }
}

/// Performance warning thresholds
#[derive(Debug, Clone)]
pub struct PerformanceThresholds {
    pub max_duration_ms: f64,
    pub min_cache_hit_rate: f64,
}

impl Default for PerformanceThresholds {
    fn default() -> Self {
        Self {
            max_duration_ms: 100.0,
            min_cache_hit_rate: 0.7,
        }
    }
}

/// Collector for performance metrics with automatic cleanup
pub struct MetricsCollector {
    metrics: Arc<RwLock<Vec<PerformanceMetrics>>>,
    max_metrics: usize,
    thresholds: PerformanceThresholds,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Vec::new())),
            max_metrics: 10000,
            thresholds: PerformanceThresholds::default(),
        }
    }

    /// Create a new metrics collector with custom capacity
    pub fn with_capacity(max_metrics: usize) -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Vec::with_capacity(max_metrics))),
            max_metrics,
            thresholds: PerformanceThresholds::default(),
        }
    }

    /// Create a new metrics collector with custom thresholds
    pub fn with_thresholds(thresholds: PerformanceThresholds) -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Vec::new())),
            max_metrics: 10000,
            thresholds,
        }
    }

    /// Record a performance metric
    pub async fn record(&self, metric: PerformanceMetrics) {
        // Check for performance warnings
        self.check_performance_warnings(&metric);

        let mut metrics = self.metrics.write().await;
        metrics.push(metric);

        // Keep only the most recent metrics to prevent unbounded growth
        if metrics.len() > self.max_metrics {
            let drain_count = self.max_metrics / 2;
            metrics.drain(0..drain_count);
        }
    }

    /// Check if a metric exceeds performance thresholds and log warnings
    fn check_performance_warnings(&self, metric: &PerformanceMetrics) {
        use tracing::warn;

        // Check evaluation duration
        if metric.duration_ms > self.thresholds.max_duration_ms {
            warn!(
                operation = %metric.operation,
                duration_ms = metric.duration_ms,
                threshold_ms = self.thresholds.max_duration_ms,
                "Performance warning: Operation exceeded duration threshold"
            );
        }
    }

    /// Check overall cache hit rate and log warning if below threshold
    pub async fn check_cache_hit_rate_warning(&self) {
        use tracing::warn;

        let stats = self.get_stats().await;

        if stats.total_operations > 100 && stats.cache_hit_rate < self.thresholds.min_cache_hit_rate
        {
            warn!(
                cache_hit_rate = stats.cache_hit_rate,
                threshold = self.thresholds.min_cache_hit_rate,
                total_operations = stats.total_operations,
                "Performance warning: Cache hit rate is below threshold"
            );
        }
    }

    /// Get aggregated statistics from collected metrics
    pub async fn get_stats(&self) -> MetricsStats {
        let metrics = self.metrics.read().await;

        if metrics.is_empty() {
            return MetricsStats::default();
        }

        let total = metrics.len();

        // Calculate average duration
        let total_duration: f64 = metrics.iter().map(|m| m.duration_ms).sum();
        let avg_duration = total_duration / total as f64;

        // Calculate percentiles
        let mut durations: Vec<f64> = metrics.iter().map(|m| m.duration_ms).collect();
        durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Use standard percentile formula: index = percentile * (n - 1)
        let p95_index = (0.95 * (total as f64 - 1.0)).round() as usize;
        let p99_index = (0.99 * (total as f64 - 1.0)).round() as usize;
        let p95_duration = durations.get(p95_index).copied().unwrap_or(0.0);
        let p99_duration = durations.get(p99_index).copied().unwrap_or(0.0);

        // Calculate cache hit rate
        let cache_hits = metrics.iter().filter(|m| m.cache_hit).count();
        let cache_hit_rate = cache_hits as f64 / total as f64;

        // Calculate total DB queries
        let total_db_queries: u64 = metrics.iter().map(|m| m.db_queries as u64).sum();

        MetricsStats {
            total_operations: total,
            avg_duration_ms: avg_duration,
            p95_duration_ms: p95_duration,
            p99_duration_ms: p99_duration,
            cache_hit_rate,
            total_db_queries,
        }
    }

    /// Get metrics for a specific operation
    pub async fn get_operation_stats(&self, operation: &str) -> MetricsStats {
        let metrics = self.metrics.read().await;

        let filtered: Vec<_> = metrics
            .iter()
            .filter(|m| m.operation == operation)
            .cloned()
            .collect();

        if filtered.is_empty() {
            return MetricsStats::default();
        }

        let total = filtered.len();

        // Calculate average duration
        let total_duration: f64 = filtered.iter().map(|m| m.duration_ms).sum();
        let avg_duration = total_duration / total as f64;

        // Calculate percentiles
        let mut durations: Vec<f64> = filtered.iter().map(|m| m.duration_ms).collect();
        durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Use standard percentile formula: index = percentile * (n - 1)
        let p95_index = (0.95 * (total as f64 - 1.0)).round() as usize;
        let p99_index = (0.99 * (total as f64 - 1.0)).round() as usize;
        let p95_duration = durations.get(p95_index).copied().unwrap_or(0.0);
        let p99_duration = durations.get(p99_index).copied().unwrap_or(0.0);

        // Calculate cache hit rate
        let cache_hits = filtered.iter().filter(|m| m.cache_hit).count();
        let cache_hit_rate = cache_hits as f64 / total as f64;

        // Calculate total DB queries
        let total_db_queries: u64 = filtered.iter().map(|m| m.db_queries as u64).sum();

        MetricsStats {
            total_operations: total,
            avg_duration_ms: avg_duration,
            p95_duration_ms: p95_duration,
            p99_duration_ms: p99_duration,
            cache_hit_rate,
            total_db_queries,
        }
    }

    /// Clear all collected metrics
    pub async fn clear(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.clear();
    }

    /// Get the number of collected metrics
    pub async fn count(&self) -> usize {
        let metrics = self.metrics.read().await;
        metrics.len()
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collector_basic() {
        let collector = MetricsCollector::new();

        let metric = PerformanceMetrics::new("test_operation")
            .with_duration(10.5)
            .with_cache_hit(true)
            .with_db_queries(2);

        collector.record(metric).await;

        let stats = collector.get_stats().await;
        assert_eq!(stats.total_operations, 1);
        assert_eq!(stats.avg_duration_ms, 10.5);
        assert_eq!(stats.cache_hit_rate, 1.0);
    }

    #[tokio::test]
    async fn test_metrics_collector_percentiles() {
        let collector = MetricsCollector::new();

        // Record metrics with varying durations
        for i in 1..=100 {
            let metric = PerformanceMetrics::new("test_operation")
                .with_duration(i as f64)
                .with_cache_hit(i % 2 == 0);

            collector.record(metric).await;
        }

        let stats = collector.get_stats().await;
        assert_eq!(stats.total_operations, 100);
        assert_eq!(stats.avg_duration_ms, 50.5);
        assert_eq!(stats.p95_duration_ms, 95.0);
        assert_eq!(stats.p99_duration_ms, 99.0);
        assert_eq!(stats.cache_hit_rate, 0.5);
    }

    #[tokio::test]
    async fn test_metrics_collector_cleanup() {
        let collector = MetricsCollector::with_capacity(100);

        // Record more metrics than capacity
        for i in 1..=150 {
            let metric = PerformanceMetrics::new("test_operation").with_duration(i as f64);
            collector.record(metric).await;
        }

        let count = collector.count().await;
        assert!(count <= 100);
    }

    #[tokio::test]
    async fn test_operation_specific_stats() {
        let collector = MetricsCollector::new();

        // Record metrics for different operations
        for i in 1..=10 {
            let metric1 = PerformanceMetrics::new("operation_a").with_duration(i as f64);
            let metric2 = PerformanceMetrics::new("operation_b").with_duration((i * 2) as f64);

            collector.record(metric1).await;
            collector.record(metric2).await;
        }

        let stats_a = collector.get_operation_stats("operation_a").await;
        let stats_b = collector.get_operation_stats("operation_b").await;

        assert_eq!(stats_a.total_operations, 10);
        assert_eq!(stats_b.total_operations, 10);
        assert_eq!(stats_a.avg_duration_ms, 5.5);
        assert_eq!(stats_b.avg_duration_ms, 11.0);
    }
}
