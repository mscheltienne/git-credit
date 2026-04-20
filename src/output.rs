use anyhow::Result;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, ContentArrangement, Table};

use crate::cli::OutputFormat;
use crate::stats::CreditReport;

/// Render the report to stdout in the chosen format.
pub fn render(report: &CreditReport, format: &OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Table => {
            render_table(report);
            Ok(())
        }
        OutputFormat::Json => render_json(report),
    }
}

fn render_table(report: &CreditReport) {
    if report.authors.is_empty() {
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

    for author in &report.authors {
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
    println!(
        "\n{} commits walked, {} squash merges expanded",
        format_number(report.total_commits_walked),
        format_number(report.squash_merges_expanded),
    );
}

fn render_json(report: &CreditReport) -> Result<()> {
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
    use crate::stats::AuthorStats;

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
        let report = CreditReport {
            authors: vec![AuthorStats {
                name: "Alice".into(),
                email: "alice@example.com".into(),
                contributions: 5,
                prs: 2,
                additions: 100,
                deletions: 50,
            }],
            total_commits_walked: 10,
            squash_merges_expanded: 2,
        };

        // Verify it produces valid JSON by round-tripping.
        let json = serde_json::to_string_pretty(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["authors"][0]["name"], "Alice");
        assert_eq!(parsed["total_commits_walked"], 10);
    }
}
