//! Fuzzy search over indexed paths using nucleo-matcher.

use crate::index::store::FileIndex;
use crate::scanner::metadata::FileEntry;
use glob::Pattern as GlobPattern;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Matcher, Utf32Str};
use std::borrow::Cow;
use std::time::{Duration, Instant};

/// How to sort search results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMode {
    #[default]
    Relevance,
    Size,
    Date,
}

/// Search configuration options.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchOptions {
    pub match_path: bool,
    pub exact_glob: bool,
    pub extension: Option<String>,
    pub sort: SortMode,
}

/// A single scored search hit.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub entry: FileEntry,
    pub score: u32,
}

/// Outcome of a search including timing metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchOutput {
    pub results: Vec<SearchResult>,
    pub total_indexed: usize,
    pub duration: Duration,
}

/// Search the index and return ranked results.
pub fn search(
    index: &FileIndex,
    query: &str,
    max_results: usize,
    options: &SearchOptions,
) -> SearchOutput {
    let started = Instant::now();
    let total_indexed = index.len();

    if query.trim().is_empty() {
        return SearchOutput {
            results: Vec::new(),
            total_indexed,
            duration: started.elapsed(),
        };
    }

    let mut results: Vec<SearchResult> = if options.exact_glob {
        search_glob(index, query, options)
    } else {
        search_fuzzy(index, query, options)
    };

    sort_results(&mut results, options.sort);
    results.truncate(max_results);

    SearchOutput {
        results,
        total_indexed,
        duration: started.elapsed(),
    }
}

fn search_fuzzy(index: &FileIndex, query: &str, options: &SearchOptions) -> Vec<SearchResult> {
    let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut buf = Vec::new();

    index
        .iter_entries()
        .filter(|entry| !entry.is_dir)
        .filter(|entry| matches_extension(entry, options))
        .filter_map(|entry| {
            let haystack: Cow<str> = if options.match_path {
                Cow::Owned(entry.path.display().to_string())
            } else {
                Cow::Borrowed(entry.filename.as_str())
            };
            let hay = Utf32Str::new(&haystack, &mut buf);
            let score = pattern.score(hay, &mut matcher)?;
            Some(SearchResult {
                entry: entry.clone(),
                score,
            })
        })
        .collect()
}

fn search_glob(index: &FileIndex, query: &str, options: &SearchOptions) -> Vec<SearchResult> {
    let pattern = match GlobPattern::new(query) {
        Ok(pattern) => pattern,
        Err(_) => return Vec::new(),
    };

    index
        .iter_entries()
        .filter(|entry| !entry.is_dir)
        .filter(|entry| matches_extension(entry, options))
        .filter_map(|entry| {
            let target: Cow<str> = if options.match_path {
                Cow::Owned(entry.path.display().to_string())
            } else {
                Cow::Borrowed(entry.filename.as_str())
            };
            if pattern.matches(&target) {
                Some(SearchResult {
                    entry: entry.clone(),
                    score: 1000,
                })
            } else {
                None
            }
        })
        .collect()
}

fn matches_extension(entry: &FileEntry, options: &SearchOptions) -> bool {
    if let Some(ext) = &options.extension {
        entry
            .extension
            .as_ref()
            .map(|e| e.eq_ignore_ascii_case(ext.trim_start_matches('.')))
            .unwrap_or(false)
    } else {
        true
    }
}

fn sort_results(results: &mut [SearchResult], sort: SortMode) {
    match sort {
        SortMode::Relevance => {
            results.sort_by(|a, b| {
                b.score
                    .cmp(&a.score)
                    .then_with(|| a.entry.path.cmp(&b.entry.path))
            });
        }
        SortMode::Size => {
            results.sort_by(|a, b| {
                b.entry
                    .size_bytes
                    .cmp(&a.entry.size_bytes)
                    .then_with(|| b.score.cmp(&a.score))
            });
        }
        SortMode::Date => {
            results.sort_by(|a, b| {
                b.entry
                    .modified
                    .cmp(&a.entry.modified)
                    .then_with(|| b.score.cmp(&a.score))
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::store::FileIndex;
    use chrono::{Duration as ChronoDuration, Utc};
    use std::path::PathBuf;

    fn entry(filename: &str, ext: &str, size: u64, age_days: i64) -> FileEntry {
        FileEntry {
            path: PathBuf::from(filename),
            filename: filename.rsplit('/').next().unwrap_or(filename).to_string(),
            extension: Some(ext.to_string()),
            size_bytes: size,
            modified: Utc::now() - ChronoDuration::days(age_days),
            created: None,
            content_hash: None,
            hash_modified: None,
            is_dir: false,
        }
    }

    fn index_with_samples() -> FileIndex {
        let mut index = FileIndex::new();
        index.insert(entry("/tmp/assignment_final.pdf", "pdf", 1000, 1));
        index.insert(entry("/tmp/notes.txt", "txt", 200, 2));
        index.insert(entry("/tmp/assignments_draft.pdf", "pdf", 300, 3));
        index.insert(entry("/tmp/readme.md", "md", 50, 4));
        index
    }

    #[test]
    fn fuzzy_finds_partial_filename_match() {
        let index = index_with_samples();
        let output = search(&index, "assign", 10, &SearchOptions::default());
        assert!(!output.results.is_empty());
        assert!(output
            .results
            .iter()
            .any(|r| r.entry.filename.contains("assign")));
    }

    #[test]
    fn fuzzy_ranks_exact_above_substring() {
        let mut index = FileIndex::new();
        index.insert(entry("/tmp/report.pdf", "pdf", 100, 1));
        index.insert(entry("/tmp/my_report_backup.pdf", "pdf", 100, 1));

        let output = search(&index, "report.pdf", 10, &SearchOptions::default());
        assert_eq!(output.results.len(), 2);
        assert_eq!(output.results[0].entry.filename, "report.pdf");
        assert!(output.results[0].score >= output.results[1].score);
    }

    #[test]
    fn exact_glob_filters_by_extension_pattern() {
        let index = index_with_samples();
        let output = search(
            &index,
            "*.pdf",
            10,
            &SearchOptions {
                exact_glob: true,
                ..Default::default()
            },
        );
        assert_eq!(output.results.len(), 2);
        assert!(output
            .results
            .iter()
            .all(|r| r.entry.extension.as_deref() == Some("pdf")));
    }

    #[test]
    fn extension_filter_limits_results() {
        let index = index_with_samples();
        let output = search(
            &index,
            "a",
            10,
            &SearchOptions {
                extension: Some("pdf".to_string()),
                ..Default::default()
            },
        );
        assert!(!output.results.is_empty());
        assert!(output
            .results
            .iter()
            .all(|r| r.entry.extension.as_deref() == Some("pdf")));
    }

    #[test]
    fn empty_query_returns_no_results() {
        let index = index_with_samples();
        let output = search(&index, "   ", 10, &SearchOptions::default());
        assert!(output.results.is_empty());
    }

    #[test]
    fn sort_by_size_orders_largest_first() {
        let index = index_with_samples();
        let output = search(
            &index,
            "a",
            10,
            &SearchOptions {
                sort: SortMode::Size,
                ..Default::default()
            },
        );
        assert!(output.results.len() >= 2);
        assert!(output.results[0].entry.size_bytes >= output.results[1].entry.size_bytes);
    }
}
