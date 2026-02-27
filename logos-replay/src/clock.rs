//! Logical clocks for causal ordering.
//!
//! Provides both a simple Lamport clock and a vector clock for
//! multi-site concurrency tracking. These are used inside `OpMetadata`
//! to establish causal ordering of operations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A Lamport logical clock — single monotonic counter.
///
/// Each site maintains its own Lamport clock. On every local event
/// the clock ticks. When receiving a remote event, the clock merges
/// (max + 1) to ensure the local counter always exceeds all known
/// remote values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LamportClock {
    /// Current counter value.
    pub counter: u64,
    /// Site identifier (e.g., user or peer ID).
    pub site_id: u64,
}

impl LamportClock {
    /// Create a clock starting at 0 for an anonymous site.
    pub fn new() -> Self {
        Self {
            counter: 0,
            site_id: 0,
        }
    }

    /// Create a clock for a specific site.
    pub fn for_site(site_id: u64) -> Self {
        Self {
            counter: 0,
            site_id,
        }
    }

    /// Advance the clock by one tick (local event).
    pub fn tick(&mut self) -> u64 {
        self.counter += 1;
        self.counter
    }

    /// Merge with a remote clock value (received event).
    /// Sets the counter to `max(self.counter, remote) + 1`.
    pub fn merge(&mut self, remote: u64) -> u64 {
        self.counter = self.counter.max(remote) + 1;
        self.counter
    }

    /// Get the current counter value.
    pub fn value(&self) -> u64 {
        self.counter
    }

    /// Check if this clock is "before" another in Lamport order.
    /// A lower counter means earlier; ties broken by site_id.
    pub fn happens_before(&self, other: &LamportClock) -> bool {
        self.counter < other.counter
            || (self.counter == other.counter && self.site_id < other.site_id)
    }
}

impl Default for LamportClock {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialOrd for LamportClock {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LamportClock {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.counter
            .cmp(&other.counter)
            .then_with(|| self.site_id.cmp(&other.site_id))
    }
}

/// A vector clock — maps site IDs to counter values.
///
/// Used for true causal ordering in multi-peer systems.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorClock {
    entries: HashMap<u64, u64>,
}

impl VectorClock {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Tick the clock for a given site.
    pub fn tick(&mut self, site_id: u64) -> u64 {
        let entry = self.entries.entry(site_id).or_insert(0);
        *entry += 1;
        *entry
    }

    /// Get the counter for a specific site.
    pub fn get(&self, site_id: u64) -> u64 {
        self.entries.get(&site_id).copied().unwrap_or(0)
    }

    /// Merge this clock with a remote clock (point-wise max).
    pub fn merge(&mut self, other: &VectorClock) {
        for (&site, &value) in &other.entries {
            let entry = self.entries.entry(site).or_insert(0);
            *entry = (*entry).max(value);
        }
    }

    /// Determine the causal relationship between two vector clocks.
    pub fn compare(&self, other: &VectorClock) -> CausalOrder {
        let all_sites: std::collections::HashSet<u64> = self
            .entries
            .keys()
            .chain(other.entries.keys())
            .copied()
            .collect();

        let mut self_leq = true;
        let mut other_leq = true;

        for site in all_sites {
            let a = self.get(site);
            let b = other.get(site);
            if a > b {
                self_leq = false;
            }
            if b > a {
                other_leq = false;
            }
        }

        match (self_leq, other_leq) {
            (true, true) => CausalOrder::Equal,
            (true, false) => CausalOrder::Before,
            (false, true) => CausalOrder::After,
            (false, false) => CausalOrder::Concurrent,
        }
    }

    /// Number of sites tracked.
    pub fn site_count(&self) -> usize {
        self.entries.len()
    }

    /// All known (site, counter) pairs.
    pub fn entries(&self) -> impl Iterator<Item = (&u64, &u64)> {
        self.entries.iter()
    }

    /// Check if all entries in this clock dominate the other.
    pub fn dominates(&self, other: &VectorClock) -> bool {
        matches!(self.compare(other), CausalOrder::After | CausalOrder::Equal)
    }
}

impl Default for VectorClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Causal ordering result between two vector clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CausalOrder {
    /// Self happened strictly before other.
    Before,
    /// Self happened strictly after other.
    After,
    /// Both are equal.
    Equal,
    /// Neither dominates — concurrent/conflicting.
    Concurrent,
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Lamport Clock ────────────────────────────────────────────────

    #[test]
    fn lamport_new() {
        let c = LamportClock::new();
        assert_eq!(c.counter, 0);
        assert_eq!(c.site_id, 0);
    }

    #[test]
    fn lamport_for_site() {
        let c = LamportClock::for_site(42);
        assert_eq!(c.site_id, 42);
        assert_eq!(c.counter, 0);
    }

    #[test]
    fn lamport_tick() {
        let mut c = LamportClock::new();
        assert_eq!(c.tick(), 1);
        assert_eq!(c.tick(), 2);
        assert_eq!(c.tick(), 3);
        assert_eq!(c.value(), 3);
    }

    #[test]
    fn lamport_merge_higher() {
        let mut c = LamportClock::for_site(1);
        c.tick(); // 1
        c.tick(); // 2
        let new = c.merge(10); // max(2, 10) + 1 = 11
        assert_eq!(new, 11);
    }

    #[test]
    fn lamport_merge_lower() {
        let mut c = LamportClock::for_site(1);
        for _ in 0..10 {
            c.tick();
        }
        let new = c.merge(3); // max(10, 3) + 1 = 11
        assert_eq!(new, 11);
    }

    #[test]
    fn lamport_happens_before() {
        let mut a = LamportClock::for_site(1);
        let mut b = LamportClock::for_site(2);
        a.tick(); // counter=1
        b.tick();
        b.tick(); // counter=2
        assert!(a.happens_before(&b));
        assert!(!b.happens_before(&a));
    }

    #[test]
    fn lamport_happens_before_tie() {
        let mut a = LamportClock::for_site(1);
        let mut b = LamportClock::for_site(2);
        a.tick();
        b.tick();
        // Both counter=1, site 1 < site 2
        assert!(a.happens_before(&b));
        assert!(!b.happens_before(&a));
    }

    #[test]
    fn lamport_ord() {
        let mut a = LamportClock::for_site(1);
        let mut b = LamportClock::for_site(2);
        a.tick();
        b.tick();
        b.tick();
        assert!(a < b);
    }

    #[test]
    fn lamport_serde_roundtrip() {
        let mut c = LamportClock::for_site(7);
        c.tick();
        c.tick();
        let json = serde_json::to_string(&c).unwrap();
        let back: LamportClock = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    // ── Vector Clock ─────────────────────────────────────────────────

    #[test]
    fn vclock_new() {
        let vc = VectorClock::new();
        assert_eq!(vc.site_count(), 0);
    }

    #[test]
    fn vclock_tick() {
        let mut vc = VectorClock::new();
        assert_eq!(vc.tick(1), 1);
        assert_eq!(vc.tick(1), 2);
        assert_eq!(vc.tick(2), 1);
        assert_eq!(vc.get(1), 2);
        assert_eq!(vc.get(2), 1);
        assert_eq!(vc.get(999), 0);
    }

    #[test]
    fn vclock_merge() {
        let mut a = VectorClock::new();
        a.tick(1); // {1:1}
        a.tick(1); // {1:2}

        let mut b = VectorClock::new();
        b.tick(1); // {1:1}
        b.tick(2); // {2:1}
        b.tick(2); // {2:2}

        a.merge(&b); // {1: max(2,1)=2, 2: max(0,2)=2}
        assert_eq!(a.get(1), 2);
        assert_eq!(a.get(2), 2);
    }

    #[test]
    fn vclock_compare_before() {
        let mut a = VectorClock::new();
        a.tick(1);

        let mut b = VectorClock::new();
        b.tick(1);
        b.tick(1);
        b.tick(2);

        assert_eq!(a.compare(&b), CausalOrder::Before);
    }

    #[test]
    fn vclock_compare_after() {
        let mut a = VectorClock::new();
        a.tick(1);
        a.tick(1);
        a.tick(2);

        let mut b = VectorClock::new();
        b.tick(1);

        assert_eq!(a.compare(&b), CausalOrder::After);
    }

    #[test]
    fn vclock_compare_equal() {
        let mut a = VectorClock::new();
        a.tick(1);
        a.tick(2);

        let mut b = VectorClock::new();
        b.tick(1);
        b.tick(2);

        assert_eq!(a.compare(&b), CausalOrder::Equal);
    }

    #[test]
    fn vclock_compare_concurrent() {
        let mut a = VectorClock::new();
        a.tick(1);
        a.tick(1);

        let mut b = VectorClock::new();
        b.tick(2);
        b.tick(2);

        assert_eq!(a.compare(&b), CausalOrder::Concurrent);
    }

    #[test]
    fn vclock_dominates() {
        let mut a = VectorClock::new();
        a.tick(1);
        a.tick(2);
        a.tick(2);

        let mut b = VectorClock::new();
        b.tick(1);
        b.tick(2);

        assert!(a.dominates(&b));
        assert!(!b.dominates(&a));
    }

    #[test]
    fn vclock_serde_roundtrip() {
        let mut vc = VectorClock::new();
        vc.tick(1);
        vc.tick(2);
        let json = serde_json::to_string(&vc).unwrap();
        let back: VectorClock = serde_json::from_str(&json).unwrap();
        assert_eq!(back, vc);
    }
}
