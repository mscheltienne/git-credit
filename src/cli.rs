use std::path::PathBuf;

use clap::Parser;

/// Precise per-author contribution stats that see through squash merges.
#[derive(Parser, Debug)]
#[command(name = "git-credit", version, about)]
pub struct Cli {
    /// Path to the git repository.
    #[arg(long, default_value = ".")]
    pub repo: PathBuf,

    /// File glob patterns to exclude from stats (repeatable).
    #[arg(long = "exclude")]
    pub excludes: Vec<String>,

    /// Only include commits after this date (YYYY-MM-DD).
    #[arg(long)]
    pub since: Option<String>,

    /// Commit range (e.g. main~50..main).
    #[arg(long)]
    pub rev: Option<String>,

    /// Output format.
    #[arg(long, value_enum, default_value = "table")]
    pub format: OutputFormat,

    /// GitHub personal access token.
    #[arg(long)]
    pub token: Option<String>,

    /// Skip GitHub API lookups for squash-merge attribution.
    #[arg(long)]
    pub no_github: bool,

    /// Include bot accounts in the output (excluded by default).
    #[arg(long)]
    pub bots: bool,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    Table,
    Json,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_args() {
        let cli = Cli::try_parse_from(["git-credit"]).unwrap();
        assert_eq!(cli.repo, PathBuf::from("."));
        assert!(cli.excludes.is_empty());
        assert!(cli.since.is_none());
        assert!(cli.rev.is_none());
        assert!(!cli.no_github);
        assert!(!cli.bots);
        assert!(matches!(cli.format, OutputFormat::Table));
    }

    #[test]
    fn multiple_excludes() {
        let cli = Cli::try_parse_from(["git-credit", "--exclude", "*.lock", "--exclude", "docs/*"])
            .unwrap();
        assert_eq!(cli.excludes, vec!["*.lock", "docs/*"]);
    }

    #[test]
    fn all_options() {
        let cli = Cli::try_parse_from([
            "git-credit",
            "--repo",
            "/tmp/repo",
            "--since",
            "2025-01-01",
            "--rev",
            "main~10..main",
            "--format",
            "json",
            "--token",
            "ghp_test",
            "--no-github",
        ])
        .unwrap();
        assert_eq!(cli.repo, PathBuf::from("/tmp/repo"));
        assert_eq!(cli.since.as_deref(), Some("2025-01-01"));
        assert_eq!(cli.rev.as_deref(), Some("main~10..main"));
        assert!(matches!(cli.format, OutputFormat::Json));
        assert_eq!(cli.token.as_deref(), Some("ghp_test"));
        assert!(cli.no_github);
    }
}
