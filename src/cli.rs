use clap::Parser;

/// Precise per-author contribution stats that see through squash merges.
#[derive(Parser, Debug)]
#[command(name = "git-credit", version, about)]
pub struct Cli;
