//! Pattern matching for organization rules.

use crate::rules::engine::{Rule, RulePattern};
use crate::scanner::metadata::FileEntry;
use chrono::Utc;

/// Return the first rule that matches the entry (rules are evaluated in order).
pub fn find_matching_rule<'a>(rules: &'a [Rule], entry: &FileEntry) -> Option<&'a Rule> {
    rules.iter().find(|rule| matches(rule, entry))
}

/// Returns true when a file entry satisfies the rule pattern.
pub fn matches(rule: &Rule, entry: &FileEntry) -> bool {
    if entry.is_dir {
        return false;
    }

    match &rule.pattern {
        RulePattern::Extension(exts) => entry
            .extension
            .as_ref()
            .map(|ext| exts.iter().any(|e| e == &ext.to_ascii_lowercase()))
            .unwrap_or(false),
        RulePattern::Contains(substring) => entry
            .filename
            .to_lowercase()
            .contains(&substring.to_lowercase()),
        RulePattern::OlderThan(duration) => {
            let threshold = Utc::now() - chrono::Duration::from_std(*duration).unwrap_or_default();
            entry.modified < threshold
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::engine::{Rule, RuleAction, RulePattern};
    use chrono::{Duration as ChronoDuration, Utc};
    use std::path::PathBuf;
    use std::time::Duration;

    fn rule_with_pattern(pattern: RulePattern, label: &str) -> Rule {
        Rule {
            pattern,
            destination: PathBuf::from("/dest"),
            action: RuleAction::Move,
            label: label.to_string(),
        }
    }

    fn sample_entry(filename: &str, ext: Option<&str>, age_days: i64) -> FileEntry {
        FileEntry {
            path: PathBuf::from(filename),
            filename: filename.to_string(),
            extension: ext.map(str::to_string),
            size_bytes: 100,
            modified: Utc::now() - ChronoDuration::days(age_days),
            created: None,
            content_hash: None,
            hash_modified: None,
            is_dir: false,
        }
    }

    #[test]
    fn extension_matches_case_insensitive() {
        let rule = rule_with_pattern(RulePattern::Extension(vec!["pdf".to_string()]), "*.pdf");
        assert!(matches(&rule, &sample_entry("doc.PDF", Some("PDF"), 0)));
        assert!(!matches(&rule, &sample_entry("doc.txt", Some("txt"), 0)));
    }

    #[test]
    fn extension_supports_multiple_extensions() {
        let rule = rule_with_pattern(
            RulePattern::Extension(vec!["png".to_string(), "jpg".to_string()]),
            "*.png,*.jpg",
        );
        assert!(matches(&rule, &sample_entry("a.png", Some("png"), 0)));
        assert!(matches(&rule, &sample_entry("b.jpg", Some("jpg"), 0)));
        assert!(!matches(&rule, &sample_entry("c.gif", Some("gif"), 0)));
    }

    #[test]
    fn contains_matches_substring() {
        let rule = rule_with_pattern(RulePattern::Contains("CS341".to_string()), "contains:CS341");
        assert!(matches(
            &rule,
            &sample_entry("CS341_HW4.pdf", Some("pdf"), 0)
        ));
        assert!(!matches(&rule, &sample_entry("notes.txt", Some("txt"), 0)));
    }

    #[test]
    fn older_than_matches_by_modified_time() {
        let rule = rule_with_pattern(
            RulePattern::OlderThan(Duration::from_secs(90 * 86400)),
            "older:90d",
        );
        assert!(matches(&rule, &sample_entry("old.bin", Some("bin"), 120)));
        assert!(!matches(&rule, &sample_entry("new.bin", Some("bin"), 1)));
    }

    #[test]
    fn rule_ordering_first_match_wins() {
        let rules = vec![
            rule_with_pattern(RulePattern::Contains("draft".to_string()), "contains:draft"),
            rule_with_pattern(RulePattern::Extension(vec!["pdf".to_string()]), "*.pdf"),
        ];
        let entry = sample_entry("draft_final.pdf", Some("pdf"), 0);
        let matched = find_matching_rule(&rules, &entry).expect("match");
        assert_eq!(matched.label, "contains:draft");
    }
}
