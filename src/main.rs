use clap::Parser;
use color_eyre::Result;
use git_credit::cli::Cli;

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();
    git_credit::run(cli)
}
