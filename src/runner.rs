use std::collections::BTreeMap;
use std::time::Instant;

use chrono::Utc;
use tokio::time::timeout;

use crate::config::{Config, OnError, SectionId, SourceType};
use crate::exec::{file, shell};
use crate::model::{Briefing, BriefingConfig, BriefingSummary, Section, SourceError, SourceResult};

/// Run all enabled sources concurrently and assemble a `Briefing`.
pub async fn collect(config: &Config, config_path: &str, scope: &str) -> Briefing {
    let wall_start = Instant::now();
    let generated_at = Utc::now();

    // AC13: filter disabled sources before execution
    let enabled: Vec<_> = config.sources.iter().filter(|s| s.enabled).collect();

    // AC14: spawn all concurrently
    let default_timeout = config.defaults.timeout_sec;
    let max_output_bytes = config.defaults.max_output_bytes;

    let handles: Vec<_> = enabled
        .iter()
        .map(|source| {
            let source = (*source).clone();
            let timeout_dur = std::time::Duration::from_secs(
                source.timeout_sec.unwrap_or(default_timeout),
            );
            tokio::spawn(async move {
                match source.source_type {
                    SourceType::Shell => {
                        // Spawn child first, then timeout on wait — allows kill on timeout
                        match shell::spawn_child(&source) {
                            Err(err_result) => err_result,
                            Ok((child, pgid)) => {
                                match timeout(timeout_dur, shell::execute_child(child, &source, max_output_bytes)).await {
                                    Ok(result) => result,
                                    Err(_) => {
                                        // Timeout: kill the process group
                                        shell::kill_process_group(pgid).await;
                                        SourceResult {
                                            id: source.id.clone(),
                                            source_type: "shell".to_string(),
                                            content_type: shell::format_to_content_type(&source.format),
                                            trust: "untrusted".to_string(),
                                            status: "error".to_string(),
                                            duration_ms: timeout_dur.as_millis() as u64,
                                            data: serde_json::Value::Null,
                                            error: Some(SourceError {
                                                error_type: "timed_out".to_string(),
                                                message: format!("source '{}' timed out after {}s", source.id, timeout_dur.as_secs()),
                                                exit_code: None,
                                                stderr: String::new(),
                                            }),
                                        }
                                    }
                                }
                            }
                        }
                    }
                    SourceType::File => {
                        match timeout(timeout_dur, file::execute(&source, max_output_bytes)).await {
                            Ok(result) => result,
                            Err(_) => SourceResult {
                                id: source.id.clone(),
                                source_type: "file".to_string(),
                                content_type: shell::format_to_content_type(&source.format),
                                trust: "untrusted".to_string(),
                                status: "error".to_string(),
                                duration_ms: timeout_dur.as_millis() as u64,
                                data: serde_json::Value::Null,
                                error: Some(SourceError {
                                    error_type: "timed_out".to_string(),
                                    message: format!("source '{}' timed out", source.id),
                                    exit_code: None,
                                    stderr: String::new(),
                                }),
                            },
                        }
                    }
                }
            })
        })
        .collect();

    // Collect results preserving source order (each handle maps 1:1 to enabled[i])
    let mut results: Vec<(SectionId, SourceResult, OnError)> = Vec::new();
    for (i, handle) in handles.into_iter().enumerate() {
        let on_error = enabled[i].on_error.clone();
        let section = enabled[i].section.clone();
        let result = handle.await.unwrap_or_else(|e| {
            SourceResult {
                id: enabled[i].id.clone(),
                source_type: "shell".to_string(),
                content_type: "text".to_string(),
                trust: "untrusted".to_string(),
                status: "error".to_string(),
                duration_ms: 0,
                data: serde_json::Value::Null,
                error: Some(SourceError {
                    error_type: "command_failed".to_string(),
                    message: format!("task panicked: {}", e),
                    exit_code: None,
                    stderr: String::new(),
                }),
            }
        });
        results.push((section, result, on_error));
    }

    // Classify results and apply on_error policy
    let mut sources_ok: usize = 0;
    let mut sources_failed: usize = 0;
    let mut sources_timed_out: usize = 0;
    let mut partial = false;

    // Group by SectionId using BTreeMap (SectionId derives Ord → deterministic order AC20)
    let mut section_map: BTreeMap<SectionId, Vec<SourceResult>> = BTreeMap::new();

    for (section_id, result, on_error) in results {
        let is_timed_out = result
            .error
            .as_ref()
            .map(|e| e.error_type == "timed_out")
            .unwrap_or(false);
        let is_error = result.status == "error";
        let is_bad = is_error || is_timed_out;

        // AC17: on_error='omit' — completely exclude from output AND summary
        if is_bad && on_error == OnError::Omit {
            continue;
        }

        // Count into summary (AC21/AC22)
        if is_timed_out {
            sources_timed_out += 1;
            partial = true; // AC23
        } else if is_error {
            sources_failed += 1;
            partial = true; // AC23
        } else {
            sources_ok += 1;
        }

        // AC18: on_error='fail' — include result but partial already set
        // AC19: on_error='warn' (default) — include with error details
        section_map.entry(section_id).or_default().push(result);
    }

    // AC20: build sections in fixed SectionId order (BTreeMap ensures Ord order)
    let sections: Vec<Section> = section_map
        .into_iter()
        .map(|(id, sources)| {
            let title = section_title(&id);
            Section {
                id: section_id_to_str(&id),
                title,
                sources,
            }
        })
        .collect();

    // AC21 + AC22: summary counts
    // AC21: sources_total = enabled, non-omitted sources counted in summary
    // sources_total = sources_ok + sources_failed + sources_timed_out (AC22)
    let sources_total = sources_ok + sources_failed + sources_timed_out;

    let duration_ms = wall_start.elapsed().as_millis() as u64;

    Briefing {
        schema_version: "1".to_string(),
        generated_at,
        duration_ms,
        partial,
        // AC24: config path and scope
        config: BriefingConfig {
            path: config_path.to_string(),
            scope: scope.to_string(),
        },
        summary: BriefingSummary {
            sources_total,
            sources_ok,
            sources_failed,
            sources_timed_out,
        },
        sections,
    }
}

fn section_title(id: &SectionId) -> String {
    match id {
        SectionId::Health => "Health".to_string(),
        SectionId::Actions => "Actions".to_string(),
        SectionId::Code => "Code".to_string(),
        SectionId::Comms => "Comms".to_string(),
        SectionId::Context => "Context".to_string(),
        SectionId::Ideas => "Ideas".to_string(),
    }
}

fn section_id_to_str(id: &SectionId) -> String {
    match id {
        SectionId::Health => "health".to_string(),
        SectionId::Actions => "actions".to_string(),
        SectionId::Code => "code".to_string(),
        SectionId::Comms => "comms".to_string(),
        SectionId::Context => "context".to_string(),
        SectionId::Ideas => "ideas".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Defaults, Source, SourceFormat};

    fn make_config(sources: Vec<Source>) -> Config {
        Config {
            schema_version: 1,
            defaults: Defaults {
                timeout_sec: 5,
                max_output_bytes: 1024 * 1024,
            },
            sources,
        }
    }

    fn shell_source(id: &str, args: Vec<&str>, enabled: bool, on_error: OnError) -> Source {
        Source {
            id: id.to_string(),
            section: SectionId::Code,
            source_type: SourceType::Shell,
            args: Some(args.into_iter().map(String::from).collect()),
            path: None,
            format: SourceFormat::Text,
            timeout_sec: None,
            on_error,
            enabled,
        }
    }

    #[tokio::test]
    async fn test_disabled_sources_filtered() {
        let cfg = make_config(vec![
            shell_source("enabled", vec!["echo", "hi"], true, OnError::Warn),
            shell_source("disabled", vec!["echo", "nope"], false, OnError::Warn),
        ]);
        let briefing = collect(&cfg, "/tmp/test.toml", "local").await;
        let all_ids: Vec<_> = briefing
            .sections
            .iter()
            .flat_map(|s| s.sources.iter().map(|r| r.id.as_str()))
            .collect();
        assert!(all_ids.contains(&"enabled"), "enabled source missing");
        assert!(!all_ids.contains(&"disabled"), "disabled source present");
    }

    #[tokio::test]
    async fn test_omit_on_error() {
        let cfg = make_config(vec![
            shell_source(
                "failing",
                vec!["__no_such_bin_recon__"],
                true,
                OnError::Omit,
            ),
        ]);
        let briefing = collect(&cfg, "/tmp/test.toml", "local").await;
        let all_ids: Vec<_> = briefing
            .sections
            .iter()
            .flat_map(|s| s.sources.iter().map(|r| r.id.as_str()))
            .collect();
        assert!(all_ids.is_empty(), "omit source should not appear in output");
        assert_eq!(briefing.summary.sources_total, 0, "omitted source not in total");
    }

    #[tokio::test]
    async fn test_warn_on_error_includes_result() {
        let cfg = make_config(vec![shell_source(
            "failing",
            vec!["__no_such_bin_recon__"],
            true,
            OnError::Warn,
        )]);
        let briefing = collect(&cfg, "/tmp/test.toml", "local").await;
        let all_ids: Vec<_> = briefing
            .sections
            .iter()
            .flat_map(|s| s.sources.iter().map(|r| r.id.as_str()))
            .collect();
        assert!(all_ids.contains(&"failing"), "warn source should appear");
        assert!(briefing.partial, "partial should be true on error");
    }

    #[tokio::test]
    async fn test_summary_counts() {
        let cfg = make_config(vec![
            shell_source("ok1", vec!["echo", "hi"], true, OnError::Warn),
            shell_source("fail1", vec!["false"], true, OnError::Warn),
        ]);
        let briefing = collect(&cfg, "/tmp/test.toml", "local").await;
        assert_eq!(briefing.summary.sources_ok, 1);
        assert_eq!(briefing.summary.sources_failed, 1);
        assert_eq!(
            briefing.summary.sources_total,
            briefing.summary.sources_ok
                + briefing.summary.sources_failed
                + briefing.summary.sources_timed_out
        );
    }

    #[tokio::test]
    async fn test_section_ordering() {
        let mut src_health = shell_source("h", vec!["echo", "1"], true, OnError::Warn);
        src_health.section = SectionId::Health;
        let mut src_ideas = shell_source("i", vec!["echo", "2"], true, OnError::Warn);
        src_ideas.section = SectionId::Ideas;
        let mut src_code = shell_source("c", vec!["echo", "3"], true, OnError::Warn);
        src_code.section = SectionId::Code;
        let cfg = make_config(vec![src_ideas, src_code, src_health]);
        let briefing = collect(&cfg, "/tmp/test.toml", "local").await;
        let ids: Vec<_> = briefing.sections.iter().map(|s| s.id.as_str()).collect();
        // AC20: Health < Code < Ideas
        let health_pos = ids.iter().position(|&x| x == "health").unwrap();
        let code_pos = ids.iter().position(|&x| x == "code").unwrap();
        let ideas_pos = ids.iter().position(|&x| x == "ideas").unwrap();
        assert!(health_pos < code_pos, "health before code");
        assert!(code_pos < ideas_pos, "code before ideas");
    }

    #[tokio::test]
    async fn test_config_populated() {
        let cfg = make_config(vec![]);
        let briefing = collect(&cfg, "/my/config.toml", "workspace").await;
        assert_eq!(briefing.config.path, "/my/config.toml");
        assert_eq!(briefing.config.scope, "workspace");
    }

    #[tokio::test]
    async fn test_timeout() {
        let mut src = shell_source("slow", vec!["sleep", "10"], true, OnError::Warn);
        src.timeout_sec = Some(1);
        let cfg = make_config(vec![src]);
        let briefing = collect(&cfg, "/tmp/test.toml", "local").await;
        let result = &briefing.sections[0].sources[0];
        assert_eq!(result.status, "error");
        assert_eq!(result.error.as_ref().unwrap().error_type, "timed_out");
        assert!(briefing.partial);
        assert_eq!(briefing.summary.sources_timed_out, 1);
    }
}
