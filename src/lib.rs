pub mod cli;

use cli::Cli;
use color_eyre::Result;

pub fn run(_cli: Cli) -> Result<()> {
    println!("Hello, world!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_succeeds() {
        assert!(run(Cli {}).is_ok());
    }
}
