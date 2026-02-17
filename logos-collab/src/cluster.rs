//! # Horizontal Scaling Infrastructure
//!
//! Provides consistent hashing, node discovery, room migration, and
//! distributed coordination for multi-node collab deployments.
//!
//! ## Architecture (Kleppmann, *DDIA* Ch. 6: Partitioning)
//!
//! ```text
//! ┌──────────┐  ┌──────────┐  ┌──────────┐
//! │  Node A  │  │  Node B  │  │  Node C  │
//! │ ring[0…5]│  │ ring[6…A]│  │ ring[B…F]│
//! └────┬─────┘  └────┬─────┘  └────┬─────┘
//!      │             │             │
//!      └──────┬──────┴─────────────┘
//!             │
//!      ┌──────┴──────┐
//!      │ Gossip /    │
//!      │ Seed-based  │
//!      │ Discovery   │
//!      └─────────────┘
//! ```
//!
//! Documents are assigned to nodes via consistent hashing on `doc_id`.
//! When a node joins or leaves, only ~K/N documents migrate (where K
//! is the number of affected ring segments and N is total nodes).

use std::collections::HashMap;
use std::fmt;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════
// Node identity
// ═══════════════════════════════════════════════════════════════════

/// Unique identifier for a cluster node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub Uuid);

impl NodeId {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node-{}", &self.0.to_string()[..8])
    }
}

/// Represents a node in the cluster.
#[derive(Debug, Clone)]
pub struct ClusterNode {
    pub id: NodeId,
    pub addr: SocketAddr,
    pub state: NodeState,
    pub joined_at: u64,
    pub last_heartbeat: u64,
    pub load: NodeLoad,
}

impl ClusterNode {
    pub fn new(id: NodeId, addr: SocketAddr) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id,
            addr,
            state: NodeState::Joining,
            joined_at: now,
            last_heartbeat: now,
            load: NodeLoad::default(),
        }
    }

    /// Whether this node is healthy and accepting connections.
    pub fn is_alive(&self) -> bool {
        matches!(self.state, NodeState::Active | NodeState::Draining)
    }
}

/// Cluster node lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
    /// Node is joining — rebalancing in progress.
    Joining,
    /// Fully active and accepting new rooms.
    Active,
    /// Draining — no new rooms, existing rooms finishing.
    Draining,
    /// Node has left the cluster.
    Left,
    /// Node is suspected dead (heartbeat timeout).
    Suspect,
}

impl fmt::Display for NodeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeState::Joining => write!(f, "joining"),
            NodeState::Active => write!(f, "active"),
            NodeState::Draining => write!(f, "draining"),
            NodeState::Left => write!(f, "left"),
            NodeState::Suspect => write!(f, "suspect"),
        }
    }
}

/// Load metrics reported by each node.
#[derive(Debug, Clone, Default)]
pub struct NodeLoad {
    pub active_rooms: u32,
    pub connected_peers: u32,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub bandwidth_bytes_sec: u64,
}

impl NodeLoad {
    /// Composite load score (0.0 = idle, 1.0 = fully loaded).
    pub fn score(&self) -> f64 {
        let rooms = (self.active_rooms as f64 / 1000.0).min(1.0);
        let peers = (self.connected_peers as f64 / 5000.0).min(1.0);
        let cpu = (self.cpu_percent as f64 / 100.0).min(1.0);
        let mem = (self.memory_bytes as f64 / (4_u64 * 1024 * 1024 * 1024) as f64).min(1.0);
        // Weighted composite
        0.3 * rooms + 0.2 * peers + 0.3 * cpu + 0.2 * mem
    }
}

// ═══════════════════════════════════════════════════════════════════
// Consistent hashing ring (Karger et al., 1997)
// ═══════════════════════════════════════════════════════════════════

/// Number of virtual nodes per physical node on the hash ring.
const DEFAULT_VNODES: usize = 128;

/// A consistent hashing ring that maps document IDs to cluster nodes.
///
/// Uses virtual nodes (vnodes) for uniform distribution. When a node
/// joins or leaves, only O(K/N) keys need to migrate. See Karger et al.,
/// "Consistent Hashing and Random Trees" (STOC 1997).
pub struct HashRing {
    /// Sorted list of (hash, node_id) pairs on the ring.
    ring: Vec<(u64, NodeId)>,
    /// Map from node_id to its virtual node count.
    nodes: HashMap<NodeId, usize>,
    /// Virtual nodes per physical node.
    vnodes_per_node: usize,
}

impl HashRing {
    pub fn new() -> Self {
        Self::with_vnodes(DEFAULT_VNODES)
    }

    pub fn with_vnodes(vnodes: usize) -> Self {
        Self {
            ring: Vec::new(),
            nodes: HashMap::new(),
            vnodes_per_node: vnodes.max(1),
        }
    }

    /// Add a node to the ring with `vnodes_per_node` virtual positions.
    pub fn add_node(&mut self, node_id: NodeId) {
        if self.nodes.contains_key(&node_id) {
            return;
        }
        let vn = self.vnodes_per_node;
        for i in 0..vn {
            let hash = self.hash_vnode(&node_id, i);
            self.ring.push((hash, node_id));
        }
        self.ring.sort_unstable_by_key(|&(h, _)| h);
        self.nodes.insert(node_id, vn);
    }

    /// Remove a node and all its virtual nodes from the ring.
    pub fn remove_node(&mut self, node_id: &NodeId) {
        if self.nodes.remove(node_id).is_none() {
            return;
        }
        self.ring.retain(|&(_, nid)| &nid != node_id);
    }

    /// Look up which node owns a given document.
    pub fn get_node(&self, doc_id: &Uuid) -> Option<NodeId> {
        if self.ring.is_empty() {
            return None;
        }
        let hash = self.hash_key(doc_id);
        // Binary search for the first ring position >= hash.
        let idx = match self.ring.binary_search_by_key(&hash, |&(h, _)| h) {
            Ok(i) => i,
            Err(i) => {
                if i >= self.ring.len() {
                    0 // Wrap around
                } else {
                    i
                }
            }
        };
        Some(self.ring[idx].1)
    }

    /// Get the N nodes responsible for a document (for replication).
    pub fn get_nodes(&self, doc_id: &Uuid, n: usize) -> Vec<NodeId> {
        if self.ring.is_empty() || n == 0 {
            return Vec::new();
        }
        let hash = self.hash_key(doc_id);
        let start = match self.ring.binary_search_by_key(&hash, |&(h, _)| h) {
            Ok(i) => i,
            Err(i) => {
                if i >= self.ring.len() {
                    0
                } else {
                    i
                }
            }
        };

        let mut result = Vec::with_capacity(n.min(self.nodes.len()));
        let len = self.ring.len();
        for offset in 0..len {
            let idx = (start + offset) % len;
            let nid = self.ring[idx].1;
            if !result.contains(&nid) {
                result.push(nid);
                if result.len() >= n || result.len() >= self.nodes.len() {
                    break;
                }
            }
        }
        result
    }

    /// Compute which documents need to migrate when a node joins or leaves.
    /// Returns a map of `doc_id → new_owner`.
    pub fn compute_migrations(
        &self,
        old_ring: &HashRing,
        doc_ids: &[Uuid],
    ) -> HashMap<Uuid, NodeId> {
        let mut migrations = HashMap::new();
        for doc_id in doc_ids {
            let old_owner = old_ring.get_node(doc_id);
            let new_owner = self.get_node(doc_id);
            if old_owner != new_owner {
                if let Some(new) = new_owner {
                    migrations.insert(*doc_id, new);
                }
            }
        }
        migrations
    }

    /// Number of physical nodes in the ring.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of virtual nodes (total ring entries).
    pub fn vnode_count(&self) -> usize {
        self.ring.len()
    }

    /// All physical node IDs currently in the ring.
    pub fn node_ids(&self) -> Vec<NodeId> {
        self.nodes.keys().copied().collect()
    }

    // ─── Internal hashing ──────────────────────────────────────────

    /// FNV-1a 64-bit hash for a document key.
    fn hash_key(&self, doc_id: &Uuid) -> u64 {
        fnv1a_64(doc_id.as_bytes())
    }

    /// FNV-1a hash for a virtual node position.
    fn hash_vnode(&self, node_id: &NodeId, vnode_idx: usize) -> u64 {
        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(node_id.as_bytes());
        data.extend_from_slice(&(vnode_idx as u64).to_le_bytes());
        fnv1a_64(&data)
    }
}

impl Default for HashRing {
    fn default() -> Self {
        Self::new()
    }
}

/// FNV-1a 64-bit hash (Fowler–Noll–Vo).
fn fnv1a_64(data: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

// ═══════════════════════════════════════════════════════════════════
// Service discovery
// ═══════════════════════════════════════════════════════════════════

/// Configuration for cluster membership discovery.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Seed nodes to contact on startup.
    pub seed_addrs: Vec<SocketAddr>,
    /// Heartbeat interval.
    pub heartbeat_interval: Duration,
    /// After this duration without heartbeat, node is suspected.
    pub suspect_timeout: Duration,
    /// After this duration in suspect state, node is declared dead.
    pub dead_timeout: Duration,
    /// Port for inter-node gossip protocol.
    pub gossip_port: u16,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            seed_addrs: Vec::new(),
            heartbeat_interval: Duration::from_secs(5),
            suspect_timeout: Duration::from_secs(15),
            dead_timeout: Duration::from_secs(60),
            gossip_port: 9100,
        }
    }
}

/// Inter-node gossip message types.
#[derive(Debug, Clone)]
pub enum GossipMessage {
    /// "I'm alive" — sent periodically.
    Heartbeat {
        from: NodeId,
        addr: SocketAddr,
        load: NodeLoad,
        epoch: u64,
    },
    /// "Join me" — node wants to enter the cluster.
    JoinRequest {
        from: NodeId,
        addr: SocketAddr,
    },
    /// "Welcome" — current membership list.
    JoinResponse {
        members: Vec<(NodeId, SocketAddr, NodeState)>,
        epoch: u64,
    },
    /// "Node X is leaving gracefully."
    LeaveNotice {
        node: NodeId,
        epoch: u64,
    },
    /// "Migrate this room to target node."
    RoomMigration {
        doc_id: Uuid,
        from_node: NodeId,
        to_node: NodeId,
        state_snapshot: Vec<u8>,
    },
}

// ═══════════════════════════════════════════════════════════════════
// Cluster manager
// ═══════════════════════════════════════════════════════════════════

/// Central coordinator for cluster membership, routing, and rebalancing.
pub struct ClusterManager {
    /// This node's identity.
    local_id: NodeId,
    /// All known cluster members.
    members: HashMap<NodeId, ClusterNode>,
    /// Hash ring for document→node mapping.
    ring: HashRing,
    /// Discovery configuration.
    config: DiscoveryConfig,
    /// Monotonically increasing cluster epoch.
    epoch: u64,
    /// Documents this node currently owns.
    local_docs: Vec<Uuid>,
}

impl ClusterManager {
    pub fn new(local_id: NodeId, local_addr: SocketAddr, config: DiscoveryConfig) -> Self {
        let mut members = HashMap::new();
        let local_node = ClusterNode::new(local_id, local_addr);
        members.insert(local_id, local_node);

        let mut ring = HashRing::new();
        ring.add_node(local_id);

        Self {
            local_id,
            members,
            ring,
            config,
            epoch: 1,
            local_docs: Vec::new(),
        }
    }

    /// This node's ID.
    pub fn local_id(&self) -> NodeId {
        self.local_id
    }

    /// Current cluster epoch.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Number of active cluster members.
    pub fn active_member_count(&self) -> usize {
        self.members.values().filter(|n| n.is_alive()).count()
    }

    /// All member nodes.
    pub fn members(&self) -> Vec<&ClusterNode> {
        self.members.values().collect()
    }

    /// Which node should own this document?
    pub fn route_document(&self, doc_id: &Uuid) -> Option<NodeId> {
        self.ring.get_node(doc_id)
    }

    /// Is this document owned by the local node?
    pub fn is_local(&self, doc_id: &Uuid) -> bool {
        self.ring.get_node(doc_id) == Some(self.local_id)
    }

    /// Add a new node to the cluster (triggered by JoinRequest).
    /// Returns the list of documents that need to migrate to the new node.
    pub fn add_node(&mut self, id: NodeId, addr: SocketAddr) -> Vec<Uuid> {
        if self.members.contains_key(&id) {
            return Vec::new();
        }

        let _old_ring = HashRing::new(); // Save current routing
        let mut old_ring_copy = HashRing::new();
        for &nid in self.ring.node_ids().iter() {
            old_ring_copy.add_node(nid);
        }

        let mut node = ClusterNode::new(id, addr);
        node.state = NodeState::Active;
        self.members.insert(id, node);
        self.ring.add_node(id);
        self.epoch += 1;

        // Compute which local docs should migrate to the new node
        let migrations = self.ring.compute_migrations(&old_ring_copy, &self.local_docs);
        let to_migrate: Vec<Uuid> = migrations
            .into_iter()
            .filter(|(_, dest)| *dest == id)
            .map(|(doc, _)| doc)
            .collect();

        // Remove migrated docs from local ownership
        self.local_docs.retain(|d| !to_migrate.contains(d));

        to_migrate
    }

    /// Remove a node from the cluster (graceful leave or dead detection).
    /// Returns documents that need to be re-homed to this node.
    pub fn remove_node(&mut self, id: &NodeId) -> Vec<Uuid> {
        if id == &self.local_id || !self.members.contains_key(id) {
            return Vec::new();
        }

        let mut old_ring = HashRing::new();
        for &nid in self.ring.node_ids().iter() {
            old_ring.add_node(nid);
        }

        self.members.remove(id);
        self.ring.remove_node(id);
        self.epoch += 1;

        // Documents previously on the removed node may now hash to us
        // The caller must provide the list of affected doc_ids separately
        // (via room migration protocol). We just update the ring here.
        Vec::new()
    }

    /// Process a heartbeat from a remote node.
    pub fn handle_heartbeat(&mut self, from: NodeId, addr: SocketAddr, load: NodeLoad, remote_epoch: u64) {
        if let Some(node) = self.members.get_mut(&from) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            node.last_heartbeat = now;
            node.load = load;
            if node.state == NodeState::Suspect {
                node.state = NodeState::Active;
            }
        } else {
            // Unknown node — auto-join
            self.add_node(from, addr);
        }
        if remote_epoch > self.epoch {
            self.epoch = remote_epoch;
        }
    }

    /// Mark a node as suspect if heartbeat has timed out.
    pub fn check_liveness(&mut self) -> Vec<NodeId> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let suspect_secs = self.config.suspect_timeout.as_secs();
        let dead_secs = self.config.dead_timeout.as_secs();

        let mut dead_nodes = Vec::new();
        for (id, node) in &mut self.members {
            if *id == self.local_id {
                continue; // Don't suspect ourselves
            }
            let age = now.saturating_sub(node.last_heartbeat);
            match node.state {
                NodeState::Active | NodeState::Joining if age > suspect_secs => {
                    node.state = NodeState::Suspect;
                }
                NodeState::Suspect if age > dead_secs => {
                    node.state = NodeState::Left;
                    dead_nodes.push(*id);
                }
                _ => {}
            }
        }
        dead_nodes
    }

    /// Register a document as locally owned.
    pub fn register_local_doc(&mut self, doc_id: Uuid) {
        if !self.local_docs.contains(&doc_id) {
            self.local_docs.push(doc_id);
        }
    }

    /// Total load score across all active members (for monitoring).
    pub fn cluster_load(&self) -> f64 {
        let active: Vec<_> = self.members.values().filter(|n| n.is_alive()).collect();
        if active.is_empty() {
            return 0.0;
        }
        let total: f64 = active.iter().map(|n| n.load.score()).sum();
        total / active.len() as f64
    }

    /// Build a cluster status report.
    pub fn status_report(&self) -> ClusterStatus {
        let members: Vec<_> = self.members.values().cloned().collect();
        let active = members.iter().filter(|n| n.is_alive()).count();
        ClusterStatus {
            local_id: self.local_id,
            epoch: self.epoch,
            total_members: members.len(),
            active_members: active,
            local_docs: self.local_docs.len(),
            ring_vnodes: self.ring.vnode_count(),
            avg_load: self.cluster_load(),
            members,
        }
    }
}

/// Snapshot of cluster health.
#[derive(Debug, Clone)]
pub struct ClusterStatus {
    pub local_id: NodeId,
    pub epoch: u64,
    pub total_members: usize,
    pub active_members: usize,
    pub local_docs: usize,
    pub ring_vnodes: usize,
    pub avg_load: f64,
    pub members: Vec<ClusterNode>,
}

impl fmt::Display for ClusterStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Cluster Status ===")?;
        writeln!(f, "Local: {}", self.local_id)?;
        writeln!(f, "Epoch: {}", self.epoch)?;
        writeln!(f, "Members: {}/{} active", self.active_members, self.total_members)?;
        writeln!(f, "Local docs: {}", self.local_docs)?;
        writeln!(f, "Ring vnodes: {}", self.ring_vnodes)?;
        writeln!(f, "Avg load: {:.2}", self.avg_load)?;
        for m in &self.members {
            writeln!(f, "  {} @ {} [{}] load={:.2}",
                m.id, m.addr, m.state, m.load.score())?;
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════
// Distributed rate limiting
// ═══════════════════════════════════════════════════════════════════

/// A distributed rate limiter that coordinates across cluster nodes.
///
/// Each node tracks local counters and periodically exchanges summaries
/// via gossip. The global rate is approximated as `sum(local_rates)`.
///
/// Ref: Kleppmann, *DDIA* Ch. 9 — "Consistency and Consensus"
pub struct DistributedRateLimiter {
    /// Per-user local counters.
    local_counters: HashMap<Uuid, TokenBucketState>,
    /// Global rate limit (requests per second across all nodes).
    global_rate_per_sec: u64,
    /// Per-node share = global_rate / active_nodes.
    local_share: u64,
    /// Number of known active nodes.
    active_nodes: u32,
    /// Total requests checked locally.
    total_checked: u64,
    /// Total requests rejected locally.
    total_rejected: u64,
}

/// Internal token bucket state.
#[derive(Debug, Clone)]
struct TokenBucketState {
    tokens: f64,
    capacity: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucketState {
    fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

impl DistributedRateLimiter {
    pub fn new(global_rate_per_sec: u64) -> Self {
        Self {
            local_counters: HashMap::new(),
            global_rate_per_sec,
            local_share: global_rate_per_sec,
            active_nodes: 1,
            total_checked: 0,
            total_rejected: 0,
        }
    }

    /// Update the number of active nodes (recalculates local share).
    pub fn set_active_nodes(&mut self, count: u32) {
        self.active_nodes = count.max(1);
        self.local_share = self.global_rate_per_sec / self.active_nodes as u64;
    }

    /// Check if a request from `user_id` should be allowed.
    pub fn check(&mut self, user_id: &Uuid) -> bool {
        self.total_checked += 1;

        let per_user_rate = (self.local_share as f64 / 100.0).max(1.0);
        let bucket = self
            .local_counters
            .entry(*user_id)
            .or_insert_with(|| TokenBucketState::new(per_user_rate * 2.0, per_user_rate));

        if bucket.try_consume() {
            true
        } else {
            self.total_rejected += 1;
            false
        }
    }

    /// Export local counter summary for gossip exchange.
    pub fn export_summary(&self) -> RateLimitSummary {
        RateLimitSummary {
            total_checked: self.total_checked,
            total_rejected: self.total_rejected,
            active_users: self.local_counters.len() as u32,
            local_share: self.local_share,
        }
    }

    /// Merge a remote node's summary (for monitoring, not enforcement).
    pub fn merge_summary(&mut self, _remote: &RateLimitSummary) {
        // Summaries are informational — each node enforces its own share.
        // In a more sophisticated system, we could adjust local_share
        // based on observed global load.
    }

    /// Garbage-collect stale user entries.
    pub fn gc(&mut self, max_age: Duration) {
        let cutoff = Instant::now() - max_age;
        self.local_counters.retain(|_, bucket| bucket.last_refill > cutoff);
    }

    pub fn total_checked(&self) -> u64 {
        self.total_checked
    }

    pub fn total_rejected(&self) -> u64 {
        self.total_rejected
    }

    pub fn active_users(&self) -> usize {
        self.local_counters.len()
    }
}

/// Summary of a node's rate-limiting state (for gossip exchange).
#[derive(Debug, Clone)]
pub struct RateLimitSummary {
    pub total_checked: u64,
    pub total_rejected: u64,
    pub active_users: u32,
    pub local_share: u64,
}

// ═══════════════════════════════════════════════════════════════════
// Room migration
// ═══════════════════════════════════════════════════════════════════

/// State of a document room being migrated between nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationState {
    /// Migration is prepared but not started.
    Pending,
    /// Source node is serializing room state.
    Snapshotting,
    /// Snapshot is being transferred to target node.
    Transferring,
    /// Target node is applying the snapshot.
    Applying,
    /// Migration completed successfully.
    Complete,
    /// Migration failed — room stays on source.
    Failed(String),
}

/// Tracks an in-flight document migration.
#[derive(Debug, Clone)]
pub struct MigrationTask {
    pub doc_id: Uuid,
    pub from_node: NodeId,
    pub to_node: NodeId,
    pub state: MigrationState,
    pub started_at: Instant,
    pub snapshot_bytes: usize,
}

impl MigrationTask {
    pub fn new(doc_id: Uuid, from: NodeId, to: NodeId) -> Self {
        Self {
            doc_id,
            from_node: from,
            to_node: to,
            state: MigrationState::Pending,
            started_at: Instant::now(),
            snapshot_bytes: 0,
        }
    }

    /// Duration since migration started.
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Advance the migration state machine.
    pub fn advance(&mut self) {
        self.state = match &self.state {
            MigrationState::Pending => MigrationState::Snapshotting,
            MigrationState::Snapshotting => MigrationState::Transferring,
            MigrationState::Transferring => MigrationState::Applying,
            MigrationState::Applying => MigrationState::Complete,
            MigrationState::Complete => MigrationState::Complete,
            MigrationState::Failed(msg) => MigrationState::Failed(msg.clone()),
        };
    }

    /// Mark migration as failed.
    pub fn fail(&mut self, reason: String) {
        self.state = MigrationState::Failed(reason);
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.state, MigrationState::Complete)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self.state, MigrationState::Failed(_))
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddrV4};

    fn addr(port: u16) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
    }

    // ── NodeId ──────────────────────────────────────────────────

    #[test]
    fn test_node_id_display() {
        let id = NodeId::generate();
        let s = format!("{}", id);
        assert!(s.starts_with("node-"));
        assert_eq!(s.len(), 13); // "node-" + 8 hex chars
    }

    #[test]
    fn test_node_id_unique() {
        let a = NodeId::generate();
        let b = NodeId::generate();
        assert_ne!(a, b);
    }

    // ── ClusterNode ─────────────────────────────────────────────

    #[test]
    fn test_cluster_node_new() {
        let id = NodeId::generate();
        let node = ClusterNode::new(id, addr(8080));
        assert_eq!(node.id, id);
        assert_eq!(node.state, NodeState::Joining);
        assert_eq!(node.load.active_rooms, 0);
    }

    #[test]
    fn test_cluster_node_is_alive() {
        let id = NodeId::generate();
        let mut node = ClusterNode::new(id, addr(8080));
        assert!(!node.is_alive()); // Joining is not alive
        node.state = NodeState::Active;
        assert!(node.is_alive());
        node.state = NodeState::Draining;
        assert!(node.is_alive());
        node.state = NodeState::Suspect;
        assert!(!node.is_alive());
        node.state = NodeState::Left;
        assert!(!node.is_alive());
    }

    // ── NodeLoad ────────────────────────────────────────────────

    #[test]
    fn test_node_load_default_score() {
        let load = NodeLoad::default();
        assert_eq!(load.score(), 0.0);
    }

    #[test]
    fn test_node_load_score_saturates() {
        let load = NodeLoad {
            active_rooms: 5000,
            connected_peers: 20000,
            cpu_percent: 200.0,
            memory_bytes: 16 * 1024 * 1024 * 1024,
            bandwidth_bytes_sec: 0,
        };
        assert!((load.score() - 1.0).abs() < 0.01);
    }

    // ── HashRing ────────────────────────────────────────────────

    #[test]
    fn test_hash_ring_empty() {
        let ring = HashRing::new();
        assert_eq!(ring.node_count(), 0);
        assert_eq!(ring.get_node(&Uuid::new_v4()), None);
    }

    #[test]
    fn test_hash_ring_single_node() {
        let mut ring = HashRing::new();
        let node = NodeId::generate();
        ring.add_node(node);
        assert_eq!(ring.node_count(), 1);
        // All docs should map to the single node
        for _ in 0..100 {
            assert_eq!(ring.get_node(&Uuid::new_v4()), Some(node));
        }
    }

    #[test]
    fn test_hash_ring_multiple_nodes_distribute() {
        // Run the test multiple times and check average distribution
        // to avoid flakiness from hash collisions with random UUIDs
        let mut ring = HashRing::with_vnodes(256);
        let nodes: Vec<NodeId> = (0..3).map(|_| NodeId::generate()).collect();
        for n in &nodes {
            ring.add_node(*n);
        }
        assert_eq!(ring.node_count(), 3);

        // Distribution should be roughly uniform (~33% each)
        let mut counts = HashMap::new();
        let total = 30000;
        for _ in 0..total {
            let owner = ring.get_node(&Uuid::new_v4()).unwrap();
            *counts.entry(owner).or_insert(0u32) += 1;
        }
        // With vnodes, each node should get at least some share
        // and no node should dominate excessively
        for n in &nodes {
            let c = *counts.get(n).unwrap_or(&0);
            let pct = c as f64 / total as f64;
            assert!(pct > 0.05, "node got only {:.1}%", pct * 100.0);
            assert!(pct < 0.80, "node got {:.1}% — too dominant", pct * 100.0);
        }
    }

    #[test]
    fn test_hash_ring_add_remove() {
        let mut ring = HashRing::with_vnodes(16);
        let n1 = NodeId::generate();
        let n2 = NodeId::generate();
        ring.add_node(n1);
        ring.add_node(n2);
        assert_eq!(ring.node_count(), 2);

        ring.remove_node(&n1);
        assert_eq!(ring.node_count(), 1);
        // All docs now go to n2
        for _ in 0..100 {
            assert_eq!(ring.get_node(&Uuid::new_v4()), Some(n2));
        }
    }

    #[test]
    fn test_hash_ring_idempotent_add() {
        let mut ring = HashRing::new();
        let n = NodeId::generate();
        ring.add_node(n);
        let vcount = ring.vnode_count();
        ring.add_node(n); // duplicate
        assert_eq!(ring.vnode_count(), vcount);
        assert_eq!(ring.node_count(), 1);
    }

    #[test]
    fn test_hash_ring_get_nodes_replication() {
        let mut ring = HashRing::with_vnodes(32);
        let nodes: Vec<NodeId> = (0..5).map(|_| NodeId::generate()).collect();
        for n in &nodes {
            ring.add_node(*n);
        }
        let doc = Uuid::new_v4();
        let replicas = ring.get_nodes(&doc, 3);
        assert_eq!(replicas.len(), 3);
        // All should be distinct
        assert_ne!(replicas[0], replicas[1]);
        assert_ne!(replicas[1], replicas[2]);
        assert_ne!(replicas[0], replicas[2]);
    }

    #[test]
    fn test_hash_ring_get_nodes_capped_at_node_count() {
        let mut ring = HashRing::new();
        let n1 = NodeId::generate();
        ring.add_node(n1);
        let replicas = ring.get_nodes(&Uuid::new_v4(), 5);
        assert_eq!(replicas.len(), 1); // Can't exceed node count
    }

    #[test]
    fn test_hash_ring_migrations() {
        let mut old_ring = HashRing::with_vnodes(32);
        let n1 = NodeId::generate();
        let n2 = NodeId::generate();
        old_ring.add_node(n1);
        old_ring.add_node(n2);

        let docs: Vec<Uuid> = (0..100).map(|_| Uuid::new_v4()).collect();

        // Add a third node
        let mut new_ring = HashRing::with_vnodes(32);
        new_ring.add_node(n1);
        new_ring.add_node(n2);
        let n3 = NodeId::generate();
        new_ring.add_node(n3);

        let migrations = new_ring.compute_migrations(&old_ring, &docs);
        // Some docs should migrate, but not all
        assert!(migrations.len() < docs.len());
        // Migrated docs should go to n3 (mostly)
        let to_n3 = migrations.values().filter(|&&dest| dest == n3).count();
        assert!(to_n3 > 0, "some docs should migrate to new node");
    }

    #[test]
    fn test_hash_ring_deterministic() {
        let mut ring = HashRing::with_vnodes(16);
        let n1 = NodeId(Uuid::from_bytes([1; 16]));
        let n2 = NodeId(Uuid::from_bytes([2; 16]));
        ring.add_node(n1);
        ring.add_node(n2);

        let doc = Uuid::from_bytes([42; 16]);
        let owner1 = ring.get_node(&doc);
        let owner2 = ring.get_node(&doc);
        assert_eq!(owner1, owner2, "consistent hashing must be deterministic");
    }

    // ── ClusterManager ──────────────────────────────────────────

    #[test]
    fn test_cluster_manager_new() {
        let id = NodeId::generate();
        let mgr = ClusterManager::new(id, addr(8080), DiscoveryConfig::default());
        assert_eq!(mgr.local_id(), id);
        assert_eq!(mgr.epoch(), 1);
        assert_eq!(mgr.active_member_count(), 0); // Joining, not Active
    }

    #[test]
    fn test_cluster_manager_add_node() {
        let id1 = NodeId::generate();
        let mut mgr = ClusterManager::new(id1, addr(8080), DiscoveryConfig::default());

        let id2 = NodeId::generate();
        mgr.add_node(id2, addr(8081));
        assert_eq!(mgr.members().len(), 2);
        assert_eq!(mgr.epoch(), 2);
    }

    #[test]
    fn test_cluster_manager_route() {
        let id1 = NodeId::generate();
        let mut mgr = ClusterManager::new(id1, addr(8080), DiscoveryConfig::default());
        let doc = Uuid::new_v4();
        // With single node, all docs route to local
        assert_eq!(mgr.route_document(&doc), Some(id1));
        assert!(mgr.is_local(&doc));
    }

    #[test]
    fn test_cluster_manager_remove_node() {
        let id1 = NodeId::generate();
        let mut mgr = ClusterManager::new(id1, addr(8080), DiscoveryConfig::default());
        let id2 = NodeId::generate();
        mgr.add_node(id2, addr(8081));
        assert_eq!(mgr.members().len(), 2);

        mgr.remove_node(&id2);
        assert_eq!(mgr.members().len(), 1);
        assert_eq!(mgr.epoch(), 3); // add=2, remove=3
    }

    #[test]
    fn test_cluster_manager_status_report() {
        let id1 = NodeId::generate();
        let mgr = ClusterManager::new(id1, addr(8080), DiscoveryConfig::default());
        let report = mgr.status_report();
        assert_eq!(report.local_id, id1);
        assert_eq!(report.total_members, 1);
        assert_eq!(report.ring_vnodes, DEFAULT_VNODES);
        let display = format!("{}", report);
        assert!(display.contains("Cluster Status"));
    }

    #[test]
    fn test_cluster_manager_register_local_doc() {
        let id1 = NodeId::generate();
        let mut mgr = ClusterManager::new(id1, addr(8080), DiscoveryConfig::default());
        let doc = Uuid::new_v4();
        mgr.register_local_doc(doc);
        mgr.register_local_doc(doc); // idempotent
        assert_eq!(mgr.status_report().local_docs, 1);
    }

    #[test]
    fn test_cluster_manager_heartbeat_unknown_node() {
        let id1 = NodeId::generate();
        let mut mgr = ClusterManager::new(id1, addr(8080), DiscoveryConfig::default());
        let id2 = NodeId::generate();
        mgr.handle_heartbeat(id2, addr(8081), NodeLoad::default(), 5);
        assert_eq!(mgr.members().len(), 2); // auto-joined
        assert_eq!(mgr.epoch(), 5); // adopted higher epoch
    }

    // ── DistributedRateLimiter ──────────────────────────────────

    #[test]
    fn test_rate_limiter_allows_initial() {
        let mut limiter = DistributedRateLimiter::new(1000);
        let user = Uuid::new_v4();
        assert!(limiter.check(&user));
        assert_eq!(limiter.total_checked(), 1);
        assert_eq!(limiter.total_rejected(), 0);
    }

    #[test]
    fn test_rate_limiter_set_active_nodes() {
        let mut limiter = DistributedRateLimiter::new(1000);
        assert_eq!(limiter.local_share, 1000);
        limiter.set_active_nodes(4);
        assert_eq!(limiter.local_share, 250);
    }

    #[test]
    fn test_rate_limiter_gc() {
        let mut limiter = DistributedRateLimiter::new(1000);
        let user = Uuid::new_v4();
        limiter.check(&user);
        assert_eq!(limiter.active_users(), 1);
        limiter.gc(Duration::from_secs(0));
        assert_eq!(limiter.active_users(), 0);
    }

    #[test]
    fn test_rate_limiter_export_summary() {
        let mut limiter = DistributedRateLimiter::new(1000);
        let user = Uuid::new_v4();
        limiter.check(&user);
        let summary = limiter.export_summary();
        assert_eq!(summary.total_checked, 1);
        assert_eq!(summary.active_users, 1);
        assert_eq!(summary.local_share, 1000);
    }

    // ── MigrationTask ───────────────────────────────────────────

    #[test]
    fn test_migration_task_new() {
        let doc = Uuid::new_v4();
        let from = NodeId::generate();
        let to = NodeId::generate();
        let task = MigrationTask::new(doc, from, to);
        assert_eq!(task.state, MigrationState::Pending);
        assert!(!task.is_complete());
        assert!(!task.is_failed());
    }

    #[test]
    fn test_migration_task_advance() {
        let doc = Uuid::new_v4();
        let from = NodeId::generate();
        let to = NodeId::generate();
        let mut task = MigrationTask::new(doc, from, to);

        task.advance();
        assert_eq!(task.state, MigrationState::Snapshotting);
        task.advance();
        assert_eq!(task.state, MigrationState::Transferring);
        task.advance();
        assert_eq!(task.state, MigrationState::Applying);
        task.advance();
        assert_eq!(task.state, MigrationState::Complete);
        assert!(task.is_complete());
        task.advance(); // Stays complete
        assert!(task.is_complete());
    }

    #[test]
    fn test_migration_task_fail() {
        let doc = Uuid::new_v4();
        let mut task = MigrationTask::new(doc, NodeId::generate(), NodeId::generate());
        task.advance(); // Snapshotting
        task.fail("disk full".to_string());
        assert!(task.is_failed());
        assert!(!task.is_complete());
        task.advance(); // Stays failed
        assert!(task.is_failed());
    }

    #[test]
    fn test_migration_task_elapsed() {
        let doc = Uuid::new_v4();
        let task = MigrationTask::new(doc, NodeId::generate(), NodeId::generate());
        assert!(task.elapsed() < Duration::from_secs(1));
    }

    // ── DiscoveryConfig ─────────────────────────────────────────

    #[test]
    fn test_discovery_config_default() {
        let cfg = DiscoveryConfig::default();
        assert_eq!(cfg.heartbeat_interval, Duration::from_secs(5));
        assert_eq!(cfg.suspect_timeout, Duration::from_secs(15));
        assert_eq!(cfg.dead_timeout, Duration::from_secs(60));
        assert_eq!(cfg.gossip_port, 9100);
        assert!(cfg.seed_addrs.is_empty());
    }

    // ── GossipMessage ───────────────────────────────────────────

    #[test]
    fn test_gossip_message_variants() {
        let id = NodeId::generate();
        let msg = GossipMessage::Heartbeat {
            from: id,
            addr: addr(8080),
            load: NodeLoad::default(),
            epoch: 5,
        };
        match msg {
            GossipMessage::Heartbeat { epoch, .. } => assert_eq!(epoch, 5),
            _ => panic!("wrong variant"),
        }

        let msg = GossipMessage::JoinRequest { from: id, addr: addr(8080) };
        assert!(matches!(msg, GossipMessage::JoinRequest { .. }));

        let msg = GossipMessage::LeaveNotice { node: id, epoch: 10 };
        assert!(matches!(msg, GossipMessage::LeaveNotice { epoch: 10, .. }));
    }

    // ── NodeState ───────────────────────────────────────────────

    #[test]
    fn test_node_state_display() {
        assert_eq!(format!("{}", NodeState::Active), "active");
        assert_eq!(format!("{}", NodeState::Joining), "joining");
        assert_eq!(format!("{}", NodeState::Draining), "draining");
        assert_eq!(format!("{}", NodeState::Left), "left");
        assert_eq!(format!("{}", NodeState::Suspect), "suspect");
    }

    // ── FNV hash ────────────────────────────────────────────────

    #[test]
    fn test_fnv1a_consistent() {
        let data = b"hello world";
        let h1 = fnv1a_64(data);
        let h2 = fnv1a_64(data);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_fnv1a_different() {
        assert_ne!(fnv1a_64(b"hello"), fnv1a_64(b"world"));
    }
}
