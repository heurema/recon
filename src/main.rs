use std::path::Path;
use std::process;

use clap::Parser;

use recon::cli::{Cli, Commands};
use recon::config::Config;
use recon::{check, init, output, runner};

#[tokio::main]
async fn main() {
    // #5: handle SIGINT gracefully
    let exit_code = tokio::select! {
        code = run_app() => code,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\ninterrupted");
            130
        }
    };
    process::exit(exit_code);
}

async fn run_app() -> i32 {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run { format, section, source } => {
            cmd_run(cli.config.as_deref(), cli.verbose, &format, section.as_deref(), source.as_deref()).await
        }
        Commands::Check { source } => {
            cmd_check(cli.config.as_deref(), cli.verbose, source.as_deref())
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
    // #14: validate format
    if format != "json" && format != "text" {
        eprintln!("error: invalid format '{}'. Valid: json, text", format);
        return 2;
    }

    // #13: validate section filter
    let valid_sections = ["health", "actions", "code", "comms", "context", "ideas"];
    if let Some(sec) = section_filter {
        if !valid_sections.contains(&sec) {
            eprintln!("error: unknown section '{}'. Valid: {}", sec, valid_sections.join(", "));
            return 2;
        }
    }

    // #10: resolve config scope
    let (scope, resolved_display) = if config_path.is_some() {
        ("explicit", config_path.unwrap().to_string())
    } else if std::env::var("RECON_CONFIG").map(|v| !v.is_empty()).unwrap_or(false) {
        ("env", std::env::var("RECON_CONFIG").unwrap())
    } else {
        ("global", "~/.config/recon/briefing.toml".to_string())
    };

    let config = match Config::load(config_path.map(Path::new)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return 2;
        }
    };

    if verbose {
        eprintln!("[recon] config: {} (scope: {})", resolved_display, scope);
        eprintln!("[recon] {} sources loaded", config.sources.len());
    }

    let filtered_config = apply_filters(config, section_filter, source_filter);

    let enabled_count = filtered_config.sources.iter().filter(|s| s.enabled).count();
    if enabled_count == 0 {
        if section_filter.is_some() || source_filter.is_some() {
            eprintln!("error: no sources match the given filter");
        } else {
            eprintln!("error: no enabled sources in config");
        }
        return 3;
    }

    if verbose {
        eprintln!("[recon] collecting {} sources...", enabled_count);
    }

    let briefing = runner::collect(&filtered_config, &resolved_display, scope).await;

    if verbose {
        eprintln!(
            "[recon] done: ok={} failed={} timed_out={} ({}ms)",
            briefing.summary.sources_ok,
            briefing.summary.sources_failed,
            briefing.summary.sources_timed_out,
            briefing.duration_ms,
        );
    }

    match format {
        "text" => print!("{}", output::render_text(&briefing)),
        _ => println!("{}", output::render_json(&briefing)),
    }

    // #8: on_error=fail → exit 3
    if runner::has_fail_policy_errors(&filtered_config, &briefing) {
        return 3;
    }

    if briefing.summary.sources_ok == 0 && enabled_count > 0 {
        3 // fatal — no sources succeeded
    } else if briefing.partial {
        1 // partial — some failed
    } else {
        0
    }
}

fn cmd_check(config_path: Option<&str>, verbose: bool, source_filter: Option<&str>) -> i32 {
    let config = match Config::load(config_path.map(Path::new)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return 2;
        }
    };

    let (report, has_issues) = check::report(&config, verbose, source_filter);
    print!("{}", report);
    // #6: exit 1 when sources have issues
    if has_issues { 1 } else { 0 }
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
