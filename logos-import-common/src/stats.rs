//! Import statistics tracking.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Statistics gathered during an import operation.
#[derive(Clone, Debug)]
pub struct ImportStats {
    /// The source format name.
    pub format: String,
    /// Number of pages created.
    pub pages: usize,
    /// Total layers created.
    pub layers: usize,
    /// Layers by type (e.g. "rect" => 5, "text" => 3).
    pub layers_by_type: HashMap<String, usize>,
    /// Number of elements skipped (unsupported types).
    pub skipped: usize,
    /// Number of warnings generated.
    pub warnings: usize,
    /// Source file size in bytes.
    pub source_size: usize,
    /// Time spent parsing the source format.
    pub parse_time: Duration,
    /// Time spent converting to logos-core types.
    pub convert_time: Duration,
}

impl ImportStats {
    pub fn new(format: &str) -> Self {
        Self {
            format: format.to_string(),
            pages: 0,
            layers: 0,
            layers_by_type: HashMap::new(),
            skipped: 0,
            warnings: 0,
            source_size: 0,
            parse_time: Duration::ZERO,
            convert_time: Duration::ZERO,
        }
    }

    /// Record a layer of the given type.
    pub fn add_layer(&mut self, layer_type: &str) {
        self.layers += 1;
        *self.layers_by_type.entry(layer_type.to_string()).or_insert(0) += 1;
    }

    /// Total import time.
    pub fn total_time(&self) -> Duration {
        self.parse_time + self.convert_time
    }

    /// Format a human-readable summary.
    pub fn summary(&self) -> String {
        format!(
            "{} import: {} pages, {} layers ({} skipped) in {:.1}ms (parse {:.1}ms + convert {:.1}ms), source {} bytes",
            self.format,
            self.pages,
            self.layers,
            self.skipped,
            self.total_time().as_secs_f64() * 1000.0,
            self.parse_time.as_secs_f64() * 1000.0,
            self.convert_time.as_secs_f64() * 1000.0,
            self.source_size,
        )
    }
}

/// Timer helper for measuring import phases.
pub struct ImportTimer {
    start: Instant,
}

impl ImportTimer {
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_new() {
        let stats = ImportStats::new("svg");
        assert_eq!(stats.format, "svg");
        assert_eq!(stats.layers, 0);
        assert_eq!(stats.pages, 0);
    }

    #[test]
    fn test_stats_add_layer() {
        let mut stats = ImportStats::new("test");
        stats.add_layer("rect");
        stats.add_layer("rect");
        stats.add_layer("text");
        assert_eq!(stats.layers, 3);
        assert_eq!(stats.layers_by_type["rect"], 2);
        assert_eq!(stats.layers_by_type["text"], 1);
    }

    #[test]
    fn test_stats_summary() {
        let mut stats = ImportStats::new("svg");
        stats.pages = 1;
        stats.layers = 10;
        stats.skipped = 2;
        stats.source_size = 1024;
        let s = stats.summary();
        assert!(s.contains("svg"));
        assert!(s.contains("10 layers"));
        assert!(s.contains("2 skipped"));
    }

    #[test]
    fn test_timer() {
        let timer = ImportTimer::start();
        std::thread::sleep(Duration::from_millis(5));
        assert!(timer.elapsed().as_millis() >= 4);
    }
}
