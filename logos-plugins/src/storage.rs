//! Persistent key-value storage for plugins.
//!
//! Each plugin gets its own isolated namespace in a storage system.
//! Storage is quota-limited to prevent abuse and supports typed
//! get/set operations with serialization.
//!
//! ## Design
//!
//! - Per-plugin isolation (no cross-plugin data access)
//! - Quota enforcement (configurable per plugin)
//! - In-memory backend with serializable snapshot
//! - Supports String, Int, Float, Bool, and JSON values

use std::collections::HashMap;
use uuid::Uuid;

use crate::runtime::PluginValue;

// ── Storage Quota ────────────────────────────────────────────

/// Quota limits for a plugin's storage.
#[derive(Debug, Clone)]
pub struct StorageQuota {
    /// Maximum number of entries.
    pub max_entries: usize,
    /// Maximum total size in bytes (keys + values).
    pub max_bytes: usize,
    /// Maximum size of a single value in bytes.
    pub max_value_bytes: usize,
}

impl Default for StorageQuota {
    fn default() -> Self {
        Self {
            max_entries: 1_000,
            max_bytes: 5 * 1024 * 1024, // 5 MB
            max_value_bytes: 1024 * 1024, // 1 MB
        }
    }
}

impl StorageQuota {
    /// Restricted quota for untrusted plugins.
    pub fn restricted() -> Self {
        Self {
            max_entries: 100,
            max_bytes: 512 * 1024, // 512 KB
            max_value_bytes: 64 * 1024, // 64 KB
        }
    }

    /// Generous quota for trusted plugins.
    pub fn trusted() -> Self {
        Self {
            max_entries: 10_000,
            max_bytes: 50 * 1024 * 1024, // 50 MB
            max_value_bytes: 10 * 1024 * 1024, // 10 MB
        }
    }
}

// ── Storage Error ────────────────────────────────────────────

/// Errors that can occur during storage operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// Entry count would exceed quota.
    EntryQuotaExceeded,
    /// Total size would exceed quota.
    SizeQuotaExceeded,
    /// Single value exceeds maximum size.
    ValueTooLarge,
    /// Key not found.
    NotFound,
    /// Plugin not registered.
    PluginNotRegistered,
    /// Key is empty.
    EmptyKey,
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EntryQuotaExceeded => write!(f, "entry quota exceeded"),
            Self::SizeQuotaExceeded => write!(f, "size quota exceeded"),
            Self::ValueTooLarge => write!(f, "value too large"),
            Self::NotFound => write!(f, "key not found"),
            Self::PluginNotRegistered => write!(f, "plugin not registered"),
            Self::EmptyKey => write!(f, "empty key"),
        }
    }
}

// ── Plugin Store (per-plugin namespace) ──────────────────────

/// Per-plugin storage namespace.
#[derive(Debug)]
struct PluginStore {
    data: HashMap<String, PluginValue>,
    total_bytes: usize,
    quota: StorageQuota,
}

impl PluginStore {
    fn new(quota: StorageQuota) -> Self {
        Self {
            data: HashMap::new(),
            total_bytes: 0,
            quota,
        }
    }

    fn entry_size(key: &str, value: &PluginValue) -> usize {
        key.len() + value_size(value)
    }

    fn get(&self, key: &str) -> Option<&PluginValue> {
        self.data.get(key)
    }

    fn set(&mut self, key: &str, value: PluginValue) -> Result<(), StorageError> {
        if key.is_empty() {
            return Err(StorageError::EmptyKey);
        }

        let val_bytes = value_size(&value);
        if val_bytes > self.quota.max_value_bytes {
            return Err(StorageError::ValueTooLarge);
        }

        let new_entry_size = Self::entry_size(key, &value);

        // If key already exists, subtract old size
        let old_size = self
            .data
            .get(key)
            .map(|v| Self::entry_size(key, v))
            .unwrap_or(0);

        let projected_bytes = self.total_bytes - old_size + new_entry_size;
        if projected_bytes > self.quota.max_bytes {
            return Err(StorageError::SizeQuotaExceeded);
        }

        if !self.data.contains_key(key) && self.data.len() >= self.quota.max_entries {
            return Err(StorageError::EntryQuotaExceeded);
        }

        self.total_bytes = projected_bytes;
        self.data.insert(key.to_string(), value);
        Ok(())
    }

    fn remove(&mut self, key: &str) -> Result<PluginValue, StorageError> {
        if let Some(value) = self.data.remove(key) {
            self.total_bytes -= Self::entry_size(key, &value);
            Ok(value)
        } else {
            Err(StorageError::NotFound)
        }
    }

    fn clear(&mut self) {
        self.data.clear();
        self.total_bytes = 0;
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn keys(&self) -> Vec<String> {
        self.data.keys().cloned().collect()
    }
}

/// Estimate size of a PluginValue in bytes.
fn value_size(v: &PluginValue) -> usize {
    match v {
        PluginValue::Null => 1,
        PluginValue::Bool(_) => 1,
        PluginValue::Int(_) => 8,
        PluginValue::Float(_) => 8,
        PluginValue::String(s) => s.len(),
        PluginValue::Array(arr) => arr.iter().map(value_size).sum::<usize>() + 8,
        PluginValue::Object(map) => {
            map.iter()
                .map(|(k, v)| k.len() + value_size(v))
                .sum::<usize>()
                + 8
        }
    }
}

// ── Storage Manager ──────────────────────────────────────────

/// Central storage manager for all plugins.
///
/// Each plugin gets its own isolated namespace. Cross-plugin data
/// access is not possible by design.
pub struct StorageManager {
    stores: HashMap<Uuid, PluginStore>,
    default_quota: StorageQuota,
}

impl StorageManager {
    /// Create a new storage manager.
    pub fn new(default_quota: StorageQuota) -> Self {
        Self {
            stores: HashMap::new(),
            default_quota,
        }
    }

    /// Create with default quota.
    pub fn with_defaults() -> Self {
        Self::new(StorageQuota::default())
    }

    /// Register a plugin for storage.
    pub fn register(&mut self, plugin_id: Uuid) {
        self.stores
            .entry(plugin_id)
            .or_insert_with(|| PluginStore::new(self.default_quota.clone()));
    }

    /// Register with a custom quota.
    pub fn register_with_quota(&mut self, plugin_id: Uuid, quota: StorageQuota) {
        self.stores
            .entry(plugin_id)
            .or_insert_with(|| PluginStore::new(quota));
    }

    /// Unregister a plugin and delete its storage.
    pub fn unregister(&mut self, plugin_id: Uuid) -> bool {
        self.stores.remove(&plugin_id).is_some()
    }

    /// Get a value from a plugin's storage.
    pub fn get(&self, plugin_id: Uuid, key: &str) -> Result<Option<&PluginValue>, StorageError> {
        let store = self
            .stores
            .get(&plugin_id)
            .ok_or(StorageError::PluginNotRegistered)?;
        Ok(store.get(key))
    }

    /// Set a value in a plugin's storage.
    pub fn set(
        &mut self,
        plugin_id: Uuid,
        key: &str,
        value: PluginValue,
    ) -> Result<(), StorageError> {
        let store = self
            .stores
            .get_mut(&plugin_id)
            .ok_or(StorageError::PluginNotRegistered)?;
        store.set(key, value)
    }

    /// Remove a value from a plugin's storage.
    pub fn remove(&mut self, plugin_id: Uuid, key: &str) -> Result<PluginValue, StorageError> {
        let store = self
            .stores
            .get_mut(&plugin_id)
            .ok_or(StorageError::PluginNotRegistered)?;
        store.remove(key)
    }

    /// Clear all data for a plugin.
    pub fn clear(&mut self, plugin_id: Uuid) -> Result<(), StorageError> {
        let store = self
            .stores
            .get_mut(&plugin_id)
            .ok_or(StorageError::PluginNotRegistered)?;
        store.clear();
        Ok(())
    }

    /// List all keys for a plugin.
    pub fn keys(&self, plugin_id: Uuid) -> Result<Vec<String>, StorageError> {
        let store = self
            .stores
            .get(&plugin_id)
            .ok_or(StorageError::PluginNotRegistered)?;
        Ok(store.keys())
    }

    /// Number of entries for a plugin.
    pub fn entry_count(&self, plugin_id: Uuid) -> Result<usize, StorageError> {
        let store = self
            .stores
            .get(&plugin_id)
            .ok_or(StorageError::PluginNotRegistered)?;
        Ok(store.len())
    }

    /// Total bytes used by a plugin.
    pub fn bytes_used(&self, plugin_id: Uuid) -> Result<usize, StorageError> {
        let store = self
            .stores
            .get(&plugin_id)
            .ok_or(StorageError::PluginNotRegistered)?;
        Ok(store.total_bytes)
    }

    /// Number of registered plugins.
    pub fn plugin_count(&self) -> usize {
        self.stores.len()
    }

    /// Check if a plugin is registered.
    pub fn is_registered(&self, plugin_id: Uuid) -> bool {
        self.stores.contains_key(&plugin_id)
    }

    /// Export all data for a plugin as a JSON-compatible PluginValue.
    pub fn export(&self, plugin_id: Uuid) -> Result<PluginValue, StorageError> {
        let store = self
            .stores
            .get(&plugin_id)
            .ok_or(StorageError::PluginNotRegistered)?;
        let map: HashMap<String, PluginValue> = store.data.clone();
        Ok(PluginValue::Object(map))
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_quota_defaults() {
        let q = StorageQuota::default();
        assert_eq!(q.max_entries, 1_000);
        assert_eq!(q.max_bytes, 5 * 1024 * 1024);
    }

    #[test]
    fn storage_quota_presets() {
        let r = StorageQuota::restricted();
        assert!(r.max_entries < StorageQuota::default().max_entries);

        let t = StorageQuota::trusted();
        assert!(t.max_entries > StorageQuota::default().max_entries);
    }

    #[test]
    fn storage_error_display() {
        assert_eq!(StorageError::NotFound.to_string(), "key not found");
        assert_eq!(StorageError::EmptyKey.to_string(), "empty key");
    }

    #[test]
    fn manager_register_and_set() {
        let mut mgr = StorageManager::with_defaults();
        let id = Uuid::new_v4();

        mgr.register(id);
        assert!(mgr.is_registered(id));

        mgr.set(id, "name", PluginValue::String("test".to_string())).unwrap();
        let val = mgr.get(id, "name").unwrap();
        assert_eq!(val, Some(&PluginValue::String("test".to_string())));
    }

    #[test]
    fn manager_unregistered_plugin_errors() {
        let mgr = StorageManager::with_defaults();
        let id = Uuid::new_v4();
        assert_eq!(mgr.get(id, "key"), Err(StorageError::PluginNotRegistered));
    }

    #[test]
    fn manager_remove_key() {
        let mut mgr = StorageManager::with_defaults();
        let id = Uuid::new_v4();
        mgr.register(id);

        mgr.set(id, "x", PluginValue::Int(42)).unwrap();
        let removed = mgr.remove(id, "x").unwrap();
        assert_eq!(removed, PluginValue::Int(42));
        assert_eq!(mgr.entry_count(id).unwrap(), 0);
    }

    #[test]
    fn manager_remove_not_found() {
        let mut mgr = StorageManager::with_defaults();
        let id = Uuid::new_v4();
        mgr.register(id);
        assert_eq!(mgr.remove(id, "nope"), Err(StorageError::NotFound));
    }

    #[test]
    fn manager_entry_quota() {
        let mut mgr = StorageManager::new(StorageQuota {
            max_entries: 2,
            max_bytes: 1_000_000,
            max_value_bytes: 100_000,
        });
        let id = Uuid::new_v4();
        mgr.register(id);

        mgr.set(id, "a", PluginValue::Int(1)).unwrap();
        mgr.set(id, "b", PluginValue::Int(2)).unwrap();
        assert_eq!(
            mgr.set(id, "c", PluginValue::Int(3)),
            Err(StorageError::EntryQuotaExceeded)
        );
    }

    #[test]
    fn manager_value_too_large() {
        let mut mgr = StorageManager::new(StorageQuota {
            max_entries: 100,
            max_bytes: 1_000_000,
            max_value_bytes: 10, // 10 bytes max
        });
        let id = Uuid::new_v4();
        mgr.register(id);

        let big = PluginValue::String("a".repeat(100));
        assert_eq!(mgr.set(id, "big", big), Err(StorageError::ValueTooLarge));
    }

    #[test]
    fn manager_size_quota() {
        let mut mgr = StorageManager::new(StorageQuota {
            max_entries: 1000,
            max_bytes: 50, // 50 bytes total
            max_value_bytes: 50,
        });
        let id = Uuid::new_v4();
        mgr.register(id);

        // First entry: key "a" (1 byte) + "hello" (5 bytes) = 6 bytes
        mgr.set(id, "a", PluginValue::String("hello".to_string()))
            .unwrap();

        // Big entry that wouldn't fit
        let big = PluginValue::String("x".repeat(45));
        assert_eq!(
            mgr.set(id, "big", big),
            Err(StorageError::SizeQuotaExceeded)
        );
    }

    #[test]
    fn manager_clear_and_keys() {
        let mut mgr = StorageManager::with_defaults();
        let id = Uuid::new_v4();
        mgr.register(id);

        mgr.set(id, "x", PluginValue::Int(1)).unwrap();
        mgr.set(id, "y", PluginValue::Int(2)).unwrap();

        let mut keys = mgr.keys(id).unwrap();
        keys.sort();
        assert_eq!(keys, vec!["x", "y"]);

        mgr.clear(id).unwrap();
        assert_eq!(mgr.entry_count(id).unwrap(), 0);
        assert_eq!(mgr.bytes_used(id).unwrap(), 0);
    }

    #[test]
    fn manager_update_existing_key() {
        let mut mgr = StorageManager::with_defaults();
        let id = Uuid::new_v4();
        mgr.register(id);

        mgr.set(id, "count", PluginValue::Int(1)).unwrap();
        mgr.set(id, "count", PluginValue::Int(99)).unwrap();

        assert_eq!(mgr.entry_count(id).unwrap(), 1); // still 1 entry
        assert_eq!(mgr.get(id, "count").unwrap(), Some(&PluginValue::Int(99)));
    }

    #[test]
    fn manager_export() {
        let mut mgr = StorageManager::with_defaults();
        let id = Uuid::new_v4();
        mgr.register(id);

        mgr.set(id, "name", PluginValue::String("test".to_string())).unwrap();
        mgr.set(id, "count", PluginValue::Int(5)).unwrap();

        let exported = mgr.export(id).unwrap();
        if let PluginValue::Object(map) = exported {
            assert_eq!(map.len(), 2);
            assert_eq!(map.get("count"), Some(&PluginValue::Int(5)));
        } else {
            panic!("expected Object");
        }
    }

    #[test]
    fn manager_empty_key_rejected() {
        let mut mgr = StorageManager::with_defaults();
        let id = Uuid::new_v4();
        mgr.register(id);
        assert_eq!(
            mgr.set(id, "", PluginValue::Int(1)),
            Err(StorageError::EmptyKey)
        );
    }
}
