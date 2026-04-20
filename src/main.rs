use anyhow::Result;
use clap::Parser;
use git_credit::cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    git_credit::run(&cli)
}
