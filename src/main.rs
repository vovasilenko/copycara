//! Copycara DLP Engine — entry point.
//!
//! Parses CLI arguments and dispatches to the appropriate command module.
//! All business logic lives in sub-modules.

#![forbid(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]

mod cli;
mod commit;
mod config;
mod dlp;
mod git;
mod hooks;
mod ignore;
mod init;
mod push;
mod sync;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => init::init_command(),
        Commands::Uninstall => init::uninstall_command(),
        Commands::ProcessCommit { target_hash } => commit::process_commit_command(&target_hash),
        Commands::Sync { resume } => {
            if resume {
                sync::sync_continue()
            } else {
                sync::sync_start()
            }
        }
        Commands::Push { force, no_private } => push::push_command(force, no_private),
    }
}
