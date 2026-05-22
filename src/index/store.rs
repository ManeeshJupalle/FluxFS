//! In-memory index (`HashMap<PathBuf, FileEntry>`).

use crate::paths::path_is_under;
use crate::scanner::metadata::FileEntry;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Aggregate statistics for the file index.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexStats {
    pub total_files: usize,
    pub total_size: u64,
    pub last_scan: DateTime<Utc>,
    pub scan_duration_ms: u64,
}

/// In-memory index of scanned files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileIndex {
    entries: HashMap<PathBuf, FileEntry>,
    hash_groups: HashMap<String, Vec<PathBuf>>,
    stats: IndexStats,
}

impl Default for FileIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl FileIndex {
    /// Create an empty index.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            hash_groups: HashMap::new(),
            stats: IndexStats {
                total_files: 0,
                total_size: 0,
                last_scan: Utc::now(),
                scan_duration_ms: 0,
            },
        }
    }

    /// Build an index from scan results and update statistics.
    ///
    /// `refresh_stats` is called exactly once at the end — bulk inserts skip
    /// the per-insert stat recalculation, so building the index is O(n) in the
    /// number of entries (the previous implementation called `insert` per
    /// entry, which recomputed `total_size` on each call → O(n²)).
    pub fn from_entries(entries: Vec<FileEntry>, scan_duration_ms: u64) -> Self {
        let mut index = Self::new();
        for entry in entries {
            index.bulk_insert(entry);
        }
        index.stats.last_scan = Utc::now();
        index.stats.scan_duration_ms = scan_duration_ms;
        index.refresh_stats();
        index
    }

    /// Insert or update an entry in the index.
    pub fn insert(&mut self, entry: FileEntry) {
        if let Some(old) = self.entries.remove(&entry.path) {
            self.remove_hash_group_entry(&old);
        }
        self.add_hash_group_entry(&entry);
        self.entries.insert(entry.path.clone(), entry);
        self.refresh_stats();
    }

    /// Insert without recomputing stats. Caller must invoke
    /// [`Self::refresh_stats`] when the bulk insert is complete.
    fn bulk_insert(&mut self, entry: FileEntry) {
        if let Some(old) = self.entries.remove(&entry.path) {
            self.remove_hash_group_entry(&old);
        }
        self.add_hash_group_entry(&entry);
        self.entries.insert(entry.path.clone(), entry);
    }

    /// Remove an entry by path.
    pub fn remove(&mut self, path: &Path) {
        if let Some(old) = self.entries.remove(path) {
            self.remove_hash_group_entry(&old);
            self.refresh_stats();
        }
    }

    /// Look up an entry by path.
    pub fn get(&self, path: &Path) -> Option<&FileEntry> {
        self.entries.get(path)
    }

    /// Index statistics.
    pub fn stats(&self) -> &IndexStats {
        &self.stats
    }

    /// Indexed file paths under a directory prefix.
    pub fn file_paths_under(&self, root: &Path) -> Vec<PathBuf> {
        self.entries
            .keys()
            .filter(|path| path_is_under(path, root))
            .cloned()
            .collect()
    }

    /// Number of indexed files.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the index has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate all indexed entries.
    pub fn iter_entries(&self) -> impl Iterator<Item = &FileEntry> {
        self.entries.values()
    }

    /// Paths and entries that need (re)hashing.
    ///
    /// An entry needs hashing when:
    /// - It has never been hashed (`content_hash` is `None`), or
    /// - The file's on-disk modified time differs from the stamp recorded at
    ///   the last hash (`hash_modified`). This catches files that changed
    ///   outside the watcher's view — for example, when an event was missed
    ///   or the daemon was not running.
    ///
    /// One `stat` syscall per indexed entry is performed to detect external
    /// changes. Files that no longer exist are left as-is (the hash is kept
    /// rather than wiped, since the index may catch up via a future event).
    pub fn entries_needing_hash(&self) -> Vec<(PathBuf, FileEntry)> {
        self.entries
            .iter()
            .filter_map(|(path, entry)| {
                if entry.is_dir {
                    return None;
                }

                let current_mtime: Option<chrono::DateTime<Utc>> = std::fs::metadata(path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(chrono::DateTime::<Utc>::from);

                let needs_hash = match (&entry.content_hash, current_mtime, entry.hash_modified) {
                    // Never hashed.
                    (None, _, _) => true,
                    // Hashed previously but file disappeared — leave alone.
                    (Some(_), None, _) => false,
                    // Hashed previously, file still here, but no stamp (legacy
                    // entries from before `hash_modified` existed) → rehash.
                    (Some(_), Some(_), None) => true,
                    // Hashed previously and stamped → rehash only if mtime
                    // diverged from the stamp.
                    (Some(_), Some(curr), Some(stamp)) => curr != stamp,
                };

                if needs_hash {
                    Some((path.clone(), entry.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Apply computed hashes and rebuild duplicate lookup groups.
    ///
    /// Each update is `(path, hash, mtime_at_hash_time)`. The mtime is stored
    /// alongside the hash so that future calls to [`Self::entries_needing_hash`]
    /// can detect when the on-disk file has been modified since the cached
    /// hash was taken.
    pub fn apply_content_hashes(&mut self, updates: Vec<(PathBuf, String, chrono::DateTime<Utc>)>) {
        for (path, hash, hashed_at) in updates {
            if let Some(entry) = self.entries.get_mut(&path) {
                entry.content_hash = Some(hash);
                entry.hash_modified = Some(hashed_at);
            }
        }
        self.rebuild_hash_groups();
    }

    /// Rebuild `hash_groups` from all indexed entries.
    pub fn rebuild_hash_groups(&mut self) {
        self.hash_groups.clear();
        let entries: Vec<FileEntry> = self.entries.values().cloned().collect();
        for entry in &entries {
            self.add_hash_group_entry(entry);
        }
    }

    /// Iterate hash groups with more than one path (for duplicate detection).
    pub fn hash_groups_with_duplicates(&self) -> Vec<(String, Vec<PathBuf>)> {
        self.hash_groups
            .iter()
            .filter(|(_, paths)| paths.len() > 1)
            .map(|(hash, paths)| (hash.clone(), paths.clone()))
            .collect()
    }

    /// Groups of entries that share the same content hash (length > 1).
    #[allow(dead_code)]
    pub fn duplicates(&self) -> Vec<Vec<&FileEntry>> {
        let mut groups = Vec::new();
        for paths in self.hash_groups.values() {
            if paths.len() < 2 {
                continue;
            }
            let group: Vec<&FileEntry> = paths
                .iter()
                .filter_map(|path| self.entries.get(path))
                .collect();
            if group.len() > 1 {
                groups.push(group);
            }
        }
        groups
    }

    fn refresh_stats(&mut self) {
        self.stats.total_files = self.entries.len();
        self.stats.total_size = self.entries.values().map(|e| e.size_bytes).sum();
    }

    fn add_hash_group_entry(&mut self, entry: &FileEntry) {
        if let Some(hash) = &entry.content_hash {
            self.hash_groups
                .entry(hash.clone())
                .or_default()
                .push(entry.path.clone());
        }
    }

    fn remove_hash_group_entry(&mut self, entry: &FileEntry) {
        if let Some(hash) = &entry.content_hash {
            if let Some(paths) = self.hash_groups.get_mut(hash) {
                paths.retain(|p| p != &entry.path);
                if paths.is_empty() {
                    self.hash_groups.remove(hash);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::path::PathBuf;

    fn sample_entry(path: &str, hash: Option<&str>) -> FileEntry {
        let modified = Utc::now();
        FileEntry {
            path: PathBuf::from(path),
            filename: path.rsplit('/').next().unwrap_or(path).to_string(),
            extension: Some("txt".to_string()),
            size_bytes: 100,
            modified,
            created: None,
            content_hash: hash.map(str::to_string),
            hash_modified: hash.map(|_| modified),
            is_dir: false,
        }
    }

    #[test]
    fn insert_get_remove() {
        let mut index = FileIndex::new();
        let entry = sample_entry("/tmp/a.txt", None);
        let path = entry.path.clone();

        index.insert(entry);
        assert_eq!(index.len(), 1);
        assert!(index.get(&path).is_some());

        index.remove(&path);
        assert!(index.is_empty());
        assert!(index.get(&path).is_none());
    }

    #[test]
    fn stats_reflect_entries() {
        let mut index = FileIndex::new();
        index.insert(sample_entry("/tmp/a.txt", None));
        index.insert(FileEntry {
            size_bytes: 250,
            ..sample_entry("/tmp/b.txt", None)
        });

        assert_eq!(index.stats().total_files, 2);
        assert_eq!(index.stats().total_size, 350);
    }

    #[test]
    fn duplicates_groups_by_hash() {
        let mut index = FileIndex::new();
        index.insert(sample_entry("/tmp/a.txt", Some("abc")));
        index.insert(sample_entry("/tmp/b.txt", Some("abc")));
        index.insert(sample_entry("/tmp/c.txt", Some("xyz")));

        let groups = index.duplicates();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }
}
