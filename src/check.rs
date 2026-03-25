use crate::config::{Config, SourceType};

/// Report on each enabled source's availability.
/// Returns (report_text, has_issues).
pub fn report(config: &Config, verbose: bool, source_filter: Option<&str>) -> (String, bool) {
    let mut out = String::new();
    out.push_str("recon check\n\n");

    let enabled: Vec<_> = config.sources.iter()
        .filter(|s| s.enabled)
        .filter(|s| source_filter.map_or(true, |f| s.id == f))
        .collect();

    if enabled.is_empty() {
        out.push_str("No enabled sources found.\n");
        return (out, false);
    }

    let mut has_issues = false;

    for source in &enabled {
        let status = match source.source_type {
            SourceType::Shell => {
                let args = source.args.as_deref().unwrap_or(&[]);
                if args.is_empty() {
                    has_issues = true;
                    "error: no args".to_string()
                } else {
                    let bin = &args[0];
                    match which::which(bin) {
                        Ok(path) => {
                            if verbose {
                                eprintln!("[recon] {} found at {}", bin, path.display());
                            }
                            "ok".to_string()
                        }
                        Err(_) => {
                            has_issues = true;
                            format!("missing: binary '{}' not found in PATH", bin)
                        }
                    }
                }
            }
            SourceType::File => {
                let path = source.path.as_deref().unwrap_or("");
                let p = std::path::Path::new(path);
                if p.exists() {
                    if p.is_dir() {
                        has_issues = true;
                        format!("error: '{}' is a directory, not a file", path)
                    } else {
                        if verbose {
                            eprintln!("[recon] file {} exists", path);
                        }
                        "ok".to_string()
                    }
                } else {
                    has_issues = true;
                    format!("missing: file '{}' not found", path)
                }
            }
        };

        let status_label = if status == "ok" { "  ok  " } else { &status[..6.min(status.len())] };
        out.push_str(&format!(
            "  [{:6}] {}  (section: {}, type: {:?})\n",
            status_label,
            source.id,
            serde_json::to_string(&source.section)
                .unwrap_or_default()
                .trim_matches('"'),
            source.source_type,
        ));
    }

    (out, has_issues)
}
