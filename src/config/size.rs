//! Human-readable size string parsing for config values.

use crate::errors::{FluxError, Result};

/// Parse human-readable size strings such as `1KB`, `10MB`, `2GB`.
pub fn parse_size(value: &str) -> Result<u64> {
    let value = value.trim();
    if value.is_empty() {
        return Err(FluxError::Config("Size value cannot be empty.".to_string()));
    }

    let (number, unit) = if let Some(stripped) = value.strip_suffix("GB") {
        (stripped, "GB")
    } else if let Some(stripped) = value.strip_suffix("MB") {
        (stripped, "MB")
    } else if let Some(stripped) = value.strip_suffix("KB") {
        (stripped, "KB")
    } else if let Some(stripped) = value.strip_suffix('B') {
        (stripped, "B")
    } else {
        (value, "B")
    };

    let number: u64 = number.trim().parse().map_err(|_| {
        FluxError::Config(format!(
            "Invalid size '{value}'. Use formats like 1KB, 10MB, 2GB."
        ))
    })?;

    let multiplier = match unit {
        "GB" => 1024 * 1024 * 1024,
        "MB" => 1024 * 1024,
        "KB" => 1024,
        "B" => 1,
        _ => {
            return Err(FluxError::Config(format!(
                "Unknown size unit in '{value}'."
            )));
        }
    };

    number
        .checked_mul(multiplier)
        .ok_or_else(|| FluxError::Config(format!("Size '{value}' is too large.")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size_supports_kb_and_gb() {
        assert_eq!(parse_size("1KB").expect("kb"), 1024);
        assert_eq!(parse_size("2GB").expect("gb"), 2 * 1024 * 1024 * 1024);
    }
}
