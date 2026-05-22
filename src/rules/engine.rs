//! Rule types and organization orchestration.

use crate::errors::Result;
use crate::index::store::FileIndex;
use crate::paths::path_is_under;
use crate::reporting::activity::log_file_indexed;
use crate::rules::actions::{organize_file, OrganizeResult};
use crate::rules::matcher::find_matching_rule;
use crate::scanner::metadata::FileEntry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How a rule matches a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RulePattern {
    Extension(Vec<String>),
    Contains(String),
    OlderThan(Duration),
}

/// File operation applied when a rule matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuleAction {
    #[default]
    Move,
    #[allow(dead_code)]
    Copy,
}

/// A single organization rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub pattern: RulePattern,
    pub destination: PathBuf,
    pub action: RuleAction,
    /// Original pattern string from config (for logging and summaries).
    pub label: String,
}

/// Rules scoped to one watched directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchRuleset {
    pub watch_path: PathBuf,
    pub rules: Vec<Rule>,
}

/// Summary of an organize run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OrganizeSummary {
    pub organized: usize,
    pub skipped: usize,
    pub dry_run: usize,
    pub by_rule: HashMap<String, usize>,
}

/// Run all rules against indexed files under each watch path (first match wins per file).
pub fn organize_index(
    index: &mut FileIndex,
    watch_rulesets: &[WatchRuleset],
    dry_run: bool,
    activity_log: &Path,
) -> Result<OrganizeSummary> {
    let mut summary = OrganizeSummary::default();

    for ruleset in watch_rulesets {
        let paths: Vec<PathBuf> = index
            .file_paths_under(&ruleset.watch_path)
            .into_iter()
            .collect();

        for path in paths {
            let Some(entry) = index.get(&path).cloned() else {
                continue;
            };

            if entry.is_dir {
                continue;
            }

            let Some(rule) = find_matching_rule(&ruleset.rules, &entry) else {
                summary.skipped += 1;
                continue;
            };

            let result = organize_file(&entry, rule, dry_run, activity_log)?;

            match result {
                OrganizeResult::DryRun { .. } => {
                    summary.dry_run += 1;
                    *summary.by_rule.entry(rule.label.clone()).or_default() += 1;
                }
                OrganizeResult::Moved { from, to } | OrganizeResult::Copied { from, to } => {
                    summary.organized += 1;
                    *summary.by_rule.entry(rule.label.clone()).or_default() += 1;

                    if !dry_run {
                        let updated = FileEntry::from_path(&to)?;
                        let mut updated = updated;
                        updated.content_hash = entry.content_hash;
                        index.remove(&from);
                        index.insert(updated);
                    }
                }
                OrganizeResult::Skipped { .. } => {
                    summary.skipped += 1;
                }
            }
        }
    }

    if !dry_run && summary.organized > 0 {
        index.rebuild_hash_groups();
    }

    Ok(summary)
}

/// Find the watch ruleset that contains a file path.
pub fn ruleset_for_path<'a>(rulesets: &'a [WatchRuleset], path: &Path) -> Option<&'a WatchRuleset> {
    rulesets
        .iter()
        .find(|ruleset| path_is_under(path, &ruleset.watch_path))
}

/// Handle a new or updated file: match rules, organize, update the index.
pub fn process_new_file(
    index: &mut FileIndex,
    path: &Path,
    rulesets: &[WatchRuleset],
    dry_run: bool,
    activity_log: &Path,
) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let entry = FileEntry::from_path(path)?;
    if entry.is_dir {
        return Ok(());
    }

    if let Some(ruleset) = ruleset_for_path(rulesets, path) {
        if let Some(rule) = find_matching_rule(&ruleset.rules, &entry) {
            match organize_file(&entry, rule, dry_run, activity_log)? {
                OrganizeResult::Moved { from, to } | OrganizeResult::Copied { from, to } => {
                    if !dry_run {
                        let mut updated = FileEntry::from_path(&to)?;
                        updated.content_hash = entry.content_hash;
                        index.remove(&from);
                        index.insert(updated);
                    }
                    return Ok(());
                }
                OrganizeResult::DryRun { .. } | OrganizeResult::Skipped { .. } => {
                    return Ok(());
                }
            }
        }
    }

    if !dry_run {
        let path = entry.path.clone();
        index.insert(entry);
        log_file_indexed(activity_log, &path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::rules::build_rule;
    use crate::config::types::WatchRule;
    use crate::index::store::FileIndex;
    use crate::reporting::activity::activity_log_path;
    use crate::rules::matcher::matches;
    use chrono::{Duration as ChronoDuration, Utc};

    fn extension_rule(ext: &str, dest: &str) -> Rule {
        Rule {
            pattern: RulePattern::Extension(vec![ext.to_string()]),
            destination: PathBuf::from(dest),
            action: RuleAction::Move,
            label: format!("*.{ext}"),
        }
    }

    fn entry(name: &str, ext: Option<&str>, age_days: i64) -> FileEntry {
        FileEntry {
            path: PathBuf::from(name),
            filename: name.rsplit('/').next().unwrap_or(name).to_string(),
            extension: ext.map(str::to_string),
            size_bytes: 10,
            modified: Utc::now() - ChronoDuration::days(age_days),
            created: None,
            content_hash: None,
            hash_modified: None,
            is_dir: false,
        }
    }

    #[test]
    fn first_matching_rule_wins() {
        let rules = vec![
            extension_rule("txt", "/docs"),
            extension_rule("pdf", "/pdfs"),
        ];
        let pdf_entry = entry("report.pdf", Some("pdf"), 1);
        let matched = find_matching_rule(&rules, &pdf_entry).expect("match");
        assert_eq!(matched.label, "*.pdf");
    }

    #[test]
    fn organize_index_moves_matching_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let watch = dir.path().join("Downloads");
        let dest = dir.path().join("PDFs");
        std::fs::create_dir_all(&watch).expect("mkdir");
        let source = watch.join("doc.pdf");
        std::fs::write(&source, b"%PDF").expect("write");

        let mut index = FileIndex::new();
        index.insert(FileEntry::from_path(&source).expect("entry"));

        let ruleset = WatchRuleset {
            watch_path: watch.clone(),
            rules: vec![build_rule(&WatchRule {
                pattern: "*.pdf".to_string(),
                destination: dest.to_str().expect("utf8 path").to_string(),
            })
            .expect("rule")],
        };

        let log_path = activity_log_path(dir.path());
        let summary = organize_index(&mut index, &[ruleset], false, &log_path).expect("organize");

        assert_eq!(summary.organized, 1);
        assert!(!source.exists());
        assert!(dest.join("doc.pdf").exists());
    }

    #[test]
    fn older_than_pattern_matches_old_files() {
        let rule = Rule {
            pattern: RulePattern::OlderThan(Duration::from_secs(30 * 86400)),
            destination: PathBuf::from("/archive"),
            action: RuleAction::Move,
            label: "older:30d".to_string(),
        };
        assert!(matches(&rule, &entry("old.txt", Some("txt"), 60)));
        assert!(!matches(&rule, &entry("new.txt", Some("txt"), 1)));
    }
}
