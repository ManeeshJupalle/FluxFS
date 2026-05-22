//! Formatted search result output for the CLI.

use crate::index::search::{SearchOutput, SearchResult};
use chrono::{DateTime, Utc};
use colored::Colorize;
use std::path::Path;

/// Print search results in a human-readable layout.
pub fn print_find_results(output: &SearchOutput) {
    if output.results.is_empty() {
        println!();
        println!("  No matching files found.");
        print_footer(output);
        return;
    }

    println!();
    for result in &output.results {
        print_result_line(result);
    }

    print_footer(output);
}

fn print_result_line(result: &SearchResult) {
    let path = color_path(&result.entry.path);
    let size = format_bytes(result.entry.size_bytes);
    let modified = format_modified(result.entry.modified);

    println!("  {}  {}  {}", path, size.dimmed(), modified.dimmed());
}

fn print_footer(output: &SearchOutput) {
    let ms = output.duration.as_secs_f64() * 1000.0;
    println!();
    println!(
        "  {} results (searched {} files in {:.0}ms)",
        output.results.len(),
        output.total_indexed,
        ms
    );
}

fn color_path(path: &Path) -> String {
    let depth = path.components().count();
    let text = path.display().to_string();
    match depth % 3 {
        0 => text.white().to_string(),
        1 => text.cyan().to_string(),
        _ => text.bright_black().to_string(),
    }
}

fn format_modified(time: DateTime<Utc>) -> String {
    time.format("%b %d").to_string()
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
