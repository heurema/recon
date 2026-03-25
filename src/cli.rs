use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "recon", version = "0.1.0", about = "Agent-first context aggregator")]
pub struct Cli {
    /// Path to config file (overrides RECON_CONFIG and default paths)
    #[arg(long, global = true)]
    pub config: Option<String>,

    /// Write debug diagnostics to stderr
    #[arg(long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Collect all sources and output a briefing
    Run {
        /// Output format: json (default) or text
        #[arg(long, default_value = "json")]
        format: String,

        /// Only include sources from this section
        #[arg(long)]
        section: Option<String>,

        /// Run and output only this source id
        #[arg(long)]
        source: Option<String>,
    },

    /// Validate config and test source availability
    Check {
        /// Check a single source by id
        #[arg(long)]
        source: Option<String>,
    },

    /// Print a commented TOML config template to stdout
    Init {
        /// Print template to stdout instead of writing a file
        #[arg(long)]
        print: bool,
    },
}
