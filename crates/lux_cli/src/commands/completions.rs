//! Shell completion generation command (`lux completions`).

use clap_complete::Shell;
use miette::Result;

/// Generate shell completions for the specified shell directly to standard output.
pub fn run_completions(shell: Shell, cmd: &mut clap::Command) -> Result<()> {
    clap_complete::generate(shell, cmd, "lux", &mut std::io::stdout());
    Ok(())
}
