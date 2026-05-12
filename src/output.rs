use anyhow::Result;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, ContentArrangement, Table};

use crate::cli::OutputFormat;
use crate::stats::{Report, rollup_by_author};

/// Render the report to stdout in the chosen format.
pub fn render(report: &Report, format: &OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Table => {
            render_table(report);
            Ok(())
        }
        OutputFormat::Json => render_json(report),
    }
}

fn render_table(report: &Report) {
    let authors = rollup_by_author(report);
    if authors.is_empty() {
        println!("No contributions found.");
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Author"),
            Cell::new("Contributions"),
            Cell::new("PRs"),
            Cell::new("+"),
            Cell::new("-"),
            Cell::new("Total"),
        ]);

    for author in &authors {
        table.add_row(vec![
            Cell::new(format!("{} <{}>", author.name, author.email)),
            Cell::new(format_number(author.contributions)),
            Cell::new(format_number(author.prs)),
            Cell::new(format_number(author.additions)),
            Cell::new(format_number(author.deletions)),
            Cell::new(format_number(author.additions + author.deletions)),
        ]);
    }

    println!("{table}");

    let author_count = format_number(authors.len() as u64);
    let bots_info = if report.summary.bots_excluded > 0 {
        format!(
            " ({} bots excluded)",
            format_number(report.summary.bots_excluded)
        )
    } else {
        String::new()
    };
    println!(
        "\n{author_count} authors{bots_info}, {} commits walked, {} squash merges expanded",
        format_number(report.summary.total_commits_walked),
        format_number(report.summary.squash_merges_expanded),
    );
}

fn render_json(report: &Report) -> Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    println!("{json}");
    Ok(())
}

/// Format a number with thousand separators (e.g., 1542 → "1,542").
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(ch);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{Attribution, CommitReport, Summary};

    #[test]
    fn format_number_no_separator() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(42), "42");
        assert_eq!(format_number(999), "999");
    }

    #[test]
    fn format_number_with_separators() {
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(1_542), "1,542");
        assert_eq!(format_number(1_000_000), "1,000,000");
    }

    #[test]
    fn render_json_valid() {
        let report = Report {
            commits: vec![CommitReport {
                sha: "abc1234".into(),
                author_date: "2025-01-01T00:00:00Z".into(),
                is_squash_pr: false,
                attributions: vec![Attribution {
                    name: "Alice".into(),
                    email: "alice@example.com".into(),
                    additions: 100,
                    deletions: 50,
                    is_pr_author: false,
                }],
                accurate: true,
            }],
            summary: Summary {
                total_commits_walked: 10,
                squash_merges_expanded: 2,
                bots_excluded: 0,
            },
        };

        let json = serde_json::to_string_pretty(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["commits"][0]["sha"], "abc1234");
        assert_eq!(parsed["commits"][0]["author_date"], "2025-01-01T00:00:00Z");
        assert_eq!(parsed["commits"][0]["attributions"][0]["name"], "Alice");
        assert_eq!(parsed["summary"]["total_commits_walked"], 10);
        assert_eq!(parsed["summary"]["squash_merges_expanded"], 2);
    }
}
