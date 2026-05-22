//! Parse organization rules from config TOML.

use crate::config::types::{expand_tilde, WatchConfig};
use crate::config::types::{FluxConfig, WatchRule};
use crate::errors::{FluxError, Result};
use crate::rules::engine::{Rule, RuleAction, RulePattern, WatchRuleset};
use std::time::Duration;

/// Parse all watch paths and their rules from configuration.
pub fn watch_rulesets_from_config(config: &FluxConfig) -> Result<Vec<WatchRuleset>> {
    config.watch.iter().map(watch_ruleset_from_config).collect()
}

fn watch_ruleset_from_config(watch: &WatchConfig) -> Result<WatchRuleset> {
    let watch_path = expand_tilde(&watch.path)?;
    let rules = watch
        .rules
        .iter()
        .map(build_rule)
        .collect::<Result<Vec<Rule>>>()?;

    Ok(WatchRuleset { watch_path, rules })
}

/// Build a `Rule` from a config `WatchRule` entry.
pub fn build_rule(watch_rule: &WatchRule) -> Result<Rule> {
    let pattern = parse_rule_pattern(&watch_rule.pattern)?;
    let destination = expand_tilde(&watch_rule.destination)?;

    Ok(Rule {
        pattern,
        destination,
        action: RuleAction::Move,
        label: watch_rule.pattern.clone(),
    })
}

/// Parse a rule pattern string from config.
pub fn parse_rule_pattern(pattern: &str) -> Result<RulePattern> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err(FluxError::Config(
            "Rule pattern cannot be empty.".to_string(),
        ));
    }

    if let Some(substring) = pattern.strip_prefix("contains:") {
        let substring = substring.trim();
        if substring.is_empty() {
            return Err(FluxError::Config(
                "contains: rules require a non-empty substring.".to_string(),
            ));
        }
        return Ok(RulePattern::Contains(substring.to_string()));
    }

    if let Some(duration_str) = pattern.strip_prefix("older:") {
        let duration = parse_older_than(duration_str)?;
        return Ok(RulePattern::OlderThan(duration));
    }

    let extensions = parse_extension_pattern(pattern);
    if extensions.is_empty() {
        return Err(FluxError::Config(format!(
            "Invalid extension pattern '{pattern}'. Use formats like *.pdf or *.png,*.jpg."
        )));
    }
    Ok(RulePattern::Extension(extensions))
}

fn parse_extension_pattern(pattern: &str) -> Vec<String> {
    pattern
        .split(',')
        .map(|part| {
            let part = part.trim();
            let ext = part
                .strip_prefix("*.")
                .or_else(|| part.strip_prefix('.'))
                .unwrap_or(part);
            ext.to_ascii_lowercase()
        })
        .filter(|ext| !ext.is_empty())
        .collect()
}

fn parse_older_than(value: &str) -> Result<Duration> {
    let value = value.trim();
    if let Some(days) = value.strip_suffix('d') {
        let days: u64 = days.trim().parse().map_err(|_| {
            FluxError::Config(format!(
                "Invalid older-than value '{value}'. Use a format like older:90d."
            ))
        })?;
        return Ok(Duration::from_secs(days * 86400));
    }

    Err(FluxError::Config(format!(
        "Invalid older-than pattern '{value}'. Supported format: older:90d"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extension_pattern() {
        let pattern = parse_rule_pattern("*.pdf").expect("parse");
        assert_eq!(pattern, RulePattern::Extension(vec!["pdf".to_string()]));
    }

    #[test]
    fn parses_multi_extension_pattern() {
        let pattern = parse_rule_pattern("*.png,*.jpg,*.jpeg").expect("parse");
        assert_eq!(
            pattern,
            RulePattern::Extension(vec![
                "png".to_string(),
                "jpg".to_string(),
                "jpeg".to_string()
            ])
        );
    }

    #[test]
    fn parses_contains_pattern() {
        let pattern = parse_rule_pattern("contains:CS341").expect("parse");
        assert_eq!(pattern, RulePattern::Contains("CS341".to_string()));
    }

    #[test]
    fn parses_older_than_pattern() {
        let pattern = parse_rule_pattern("older:90d").expect("parse");
        assert_eq!(
            pattern,
            RulePattern::OlderThan(Duration::from_secs(90 * 86400))
        );
    }

    #[test]
    fn invalid_contains_returns_error() {
        let err = parse_rule_pattern("contains:").unwrap_err();
        assert!(err.to_string().contains("contains"));
    }

    #[test]
    fn default_config_builds_rulesets() {
        let cfg = FluxConfig::default();
        let rulesets = watch_rulesets_from_config(&cfg).expect("rulesets");
        assert_eq!(rulesets.len(), 1);
        assert_eq!(rulesets[0].rules.len(), 4);
    }
}
