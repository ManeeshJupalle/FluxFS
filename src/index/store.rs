//! In-memory index (`HashMap<PathBuf, FileEntry>`).

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

#[allow(dead_code)] // Methods used by tests; wired into CLI in Phases 3–6.
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
    pub fn from_entries(entries: Vec<FileEntry>, scan_duration_ms: u64) -> Self {
        let mut index = Self::new();
        for entry in entries {
            index.insert(entry);
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

    /// Number of indexed files.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the index has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Placeholder search until Phase 6 fuzzy matching.
    pub fn search(&self, query: &str) -> Vec<&FileEntry> {
        let query = query.to_lowercase();
        let mut results: Vec<&FileEntry> = self
            .entries
            .values()
            .filter(|entry| entry.filename.to_lowercase().contains(&query))
            .collect();
        results.sort_by(|a, b| a.filename.cmp(&b.filename));
        results
    }

    /// Groups of entries that share the same content hash (length > 1).
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
        FileEntry {
            path: PathBuf::from(path),
            filename: path.rsplit('/').next().unwrap_or(path).to_string(),
            extension: Some("txt".to_string()),
            size_bytes: 100,
            modified: Utc::now(),
            created: None,
            content_hash: hash.map(str::to_string),
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

    #[test]
    fn search_finds_filename_substring() {
        let mut index = FileIndex::new();
        index.insert(FileEntry {
            filename: "report_final.pdf".to_string(),
            ..sample_entry("/tmp/report_final.pdf", None)
        });
        index.insert(FileEntry {
            filename: "notes.txt".to_string(),
            ..sample_entry("/tmp/notes.txt", None)
        });

        let hits = index.search("report");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].filename, "report_final.pdf");
    }
}
