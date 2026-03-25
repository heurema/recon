use crate::config::{Config, SourceType};

/// Report on each enabled source's availability.
/// Verifies that the binary (shell) or file (file) exists.
/// Returns a human-readable text report.
pub fn report(config: &Config, verbose: bool) -> String {
    let mut out = String::new();
    out.push_str("recon check\n\n");

    let enabled: Vec<_> = config.sources.iter().filter(|s| s.enabled).collect();

    if enabled.is_empty() {
        out.push_str("No enabled sources found.\n");
        return out;
    }

    for source in &enabled {
        let status = match source.source_type {
            SourceType::Shell => {
                let args = source.args.as_deref().unwrap_or(&[]);
                if args.is_empty() {
                    "error: no args".to_string()
                } else {
                    let bin = &args[0];
                    match which::which(bin) {
                        Ok(path) => {
                            if verbose {
                                eprintln!("[verbose] {} found at {}", bin, path.display());
                            }
                            "ok".to_string()
                        }
                        Err(_) => format!("missing: binary '{}' not found in PATH", bin),
                    }
                }
            }
            SourceType::File => {
                let path = source.path.as_deref().unwrap_or("");
                if std::path::Path::new(path).exists() {
                    if verbose {
                        eprintln!("[verbose] file {} exists", path);
                    }
                    "ok".to_string()
                } else {
                    format!("missing: file '{}' not found", path)
                }
            }
        };

        out.push_str(&format!(
            "  [{:6}] {}  (section: {}, type: {:?})\n",
            status,
            source.id,
            serde_json::to_string(&source.section)
                .unwrap_or_default()
                .trim_matches('"'),
            source.source_type,
        ));
    }

    out
}
