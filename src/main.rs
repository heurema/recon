use std::path::Path;
use std::process;

use clap::Parser;

use recon::cli::{Cli, Commands};
use recon::config::Config;
use recon::{check, init, output, runner};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let exit_code = run(cli).await;
    process::exit(exit_code);
}

async fn run(cli: Cli) -> i32 {
    match cli.command {
        Commands::Run { format, section, source } => {
            cmd_run(cli.config.as_deref(), cli.verbose, &format, section.as_deref(), source.as_deref()).await
        }
        Commands::Check => {
            cmd_check(cli.config.as_deref(), cli.verbose)
        }
        Commands::Init { print } => {
            cmd_init(print)
        }
    }
}

async fn cmd_run(
    config_path: Option<&str>,
    verbose: bool,
    format: &str,
    section_filter: Option<&str>,
    source_filter: Option<&str>,
) -> i32 {
    // Load config — exit 2 on config errors
    let _config_path_str = config_path.unwrap_or("");
    let config = match Config::load(config_path.map(Path::new)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return 2;
        }
    };

    if verbose {
        eprintln!("[verbose] config loaded: {} sources", config.sources.len());
        if let Some(s) = section_filter {
            eprintln!("[verbose] section filter: {}", s);
        }
        if let Some(s) = source_filter {
            eprintln!("[verbose] source filter: {}", s);
        }
    }

    // Apply source/section filters before collection
    let filtered_config = apply_filters(config, section_filter, source_filter);

    // Exit 3 if no enabled sources remain after filtering
    let enabled_count = filtered_config.sources.iter().filter(|s| s.enabled).count();
    if enabled_count == 0 && (section_filter.is_some() || source_filter.is_some()) {
        eprintln!("error: no sources match the given filter");
        return 3;
    }

    let scope = "local";
    let resolved_path = config_path.unwrap_or("~/.config/recon/briefing.toml");

    if verbose {
        eprintln!("[verbose] collecting {} enabled sources", enabled_count);
    }

    let briefing = runner::collect(&filtered_config, resolved_path, scope).await;

    if verbose {
        eprintln!(
            "[verbose] done: ok={} failed={} timed_out={}",
            briefing.summary.sources_ok,
            briefing.summary.sources_failed,
            briefing.summary.sources_timed_out,
        );
    }

    // Render output to stdout
    match format {
        "text" => {
            print!("{}", output::render_text(&briefing));
        }
        _ => {
            // Default: json
            println!("{}", output::render_json(&briefing));
        }
    }

    // Determine exit code
    let all_enabled = filtered_config.sources.iter().filter(|s| s.enabled).count();
    if briefing.summary.sources_failed > 0 || briefing.summary.sources_timed_out > 0 {
        if briefing.summary.sources_ok == 0 && all_enabled > 0 {
            3 // fatal — no sources succeeded
        } else {
            1 // partial — some failed
        }
    } else {
        0 // all ok
    }
}

fn cmd_check(config_path: Option<&str>, verbose: bool) -> i32 {
    let config = match Config::load(config_path.map(Path::new)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return 2;
        }
    };

    let report = check::report(&config, verbose);
    print!("{}", report);
    0
}

fn cmd_init(print_flag: bool) -> i32 {
    if print_flag {
        print!("{}", init::template());
    } else {
        eprintln!("Use --print to output the template to stdout.");
        eprintln!("Example: recon init --print > ~/.config/recon/briefing.toml");
    }
    0
}

/// Filter config sources by section and/or source id.
fn apply_filters(
    mut config: recon::Config,
    section_filter: Option<&str>,
    source_filter: Option<&str>,
) -> recon::Config {
    if let Some(sec) = section_filter {
        config.sources.retain(|s| {
            let sec_str = serde_json::to_string(&s.section)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();
            sec_str == sec
        });
    }

    if let Some(src_id) = source_filter {
        config.sources.retain(|s| s.id == src_id);
    }

    config
}
