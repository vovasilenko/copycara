//! CLI argument parsing with clap.
//!
//! Defines the top-level CLI structure and all subcommands.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "copycara", version = "0.2.1", about = "Topological Git DLP Engine")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialise shadow mirror and hooks in the current repository
    Init,
    /// Completely remove Copycara from the repository (restore standard Git)
    Uninstall,
    /// Show health status of Copycara in this repository
    Status,
    /// Process a commit (called automatically via git hooks)
    ProcessCommit {
        /// Commit hash to process
        target_hash: String,
    },
    /// Synchronise with remote (Reverse Smudge)
    Sync {
        /// Continue after resolving conflicts
        #[arg(long = "continue")]
        resume: bool,
    },
    /// Safely push clean code to origin and backup to private
    Push {
        /// Force push shadow refs (uses --force-with-lease)
        #[arg(long)]
        force: bool,
        /// Skip push to private remote
        #[arg(long)]
        no_private: bool,
        /// Continue pushing to other remotes even if one fails
        #[arg(long)]
        continue_on_error: bool,
    },
}
