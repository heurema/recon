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

    let enabled: Vec<_> = config.sources.iter().filter(|s| s.enabled).collect();

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
                        match shell::spawn_child(&source) {
                            Err(err_result) => err_result,
                            Ok((child, pgid)) => {
                                match timeout(timeout_dur, shell::execute_child(child, &source, max_output_bytes)).await {
                                    Ok(result) => result,
                                    Err(_) => {
                                        shell::kill_process_group(pgid).await;
                                        // #15: status = "timed_out" not "error"
                                        SourceResult::new(
                                            source.id.clone(),
                                            "shell",
                                            shell::format_to_content_type(&source.format),
                                            "timed_out",
                                            timeout_dur.as_millis() as u64,
                                            serde_json::Value::Null,
                                            Some(SourceError {
                                                error_type: "timed_out".to_string(),
                                                message: format!("source '{}' timed out after {}s", source.id, timeout_dur.as_secs()),
                                                exit_code: None,
                                                stderr: String::new(),
                                            }),
                                        )
                                    }
                                }
                            }
                        }
                    }
                    SourceType::File => {
                        match timeout(timeout_dur, file::execute(&source, max_output_bytes)).await {
                            Ok(result) => result,
                            Err(_) => SourceResult::new(
                                source.id.clone(),
                                "file",
                                shell::format_to_content_type(&source.format),
                                "timed_out",
                                timeout_dur.as_millis() as u64,
                                serde_json::Value::Null,
                                Some(SourceError {
                                    error_type: "timed_out".to_string(),
                                    message: format!("source '{}' timed out", source.id),
                                    exit_code: None,
                                    stderr: String::new(),
                                }),
                            ),
                        }
                    }
                }
            })
        })
        .collect();

    let mut results: Vec<(SectionId, SourceResult, OnError)> = Vec::new();
    for (i, handle) in handles.into_iter().enumerate() {
        let on_error = enabled[i].on_error.clone();
        let section = enabled[i].section.clone();
        // #20: use correct source_type in panic fallback
        let src_type = match enabled[i].source_type {
            SourceType::Shell => "shell",
            SourceType::File => "file",
        };
        let result = handle.await.unwrap_or_else(|e| {
            SourceResult::new(
                enabled[i].id.clone(),
                src_type,
                "text".to_string(),
                "error",
                0,
                serde_json::Value::Null,
                Some(SourceError {
                    error_type: "command_failed".to_string(),
                    message: format!("task panicked: {}", e),
                    exit_code: None,
                    stderr: String::new(),
                }),
            )
        });
        results.push((section, result, on_error));
    }

    let mut sources_ok: usize = 0;
    let mut sources_failed: usize = 0;
    let mut sources_timed_out: usize = 0;
    let mut partial = false;
    let mut has_fail_policy_error = false;

    let mut section_map: BTreeMap<SectionId, Vec<SourceResult>> = BTreeMap::new();

    for (section_id, result, on_error) in results {
        let is_timed_out = result.status == "timed_out";
        let is_error = result.status == "error";
        let is_bad = is_error || is_timed_out;

        // on_error='omit' — completely exclude
        if is_bad && on_error == OnError::Omit {
            continue;
        }

        // #8: on_error='fail' — flag for exit code 3
        if is_bad && on_error == OnError::Fail {
            has_fail_policy_error = true;
        }

        if is_timed_out {
            sources_timed_out += 1;
            partial = true;
        } else if is_error {
            sources_failed += 1;
            partial = true;
        } else {
            sources_ok += 1;
        }

        section_map.entry(section_id).or_default().push(result);
    }

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

    let sources_total = sources_ok + sources_failed + sources_timed_out;
    let duration_ms = wall_start.elapsed().as_millis() as u64;

    Briefing {
        // #9: schema_version "0.1" per roadmap spec
        schema_version: "0.1".to_string(),
        generated_at,
        duration_ms,
        // #8: on_error=fail forces partial
        partial: partial || has_fail_policy_error,
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
        diff_mode: None,
        baseline_at: None,
    }
}

/// #8: check if any on_error=fail source errored (for exit code logic in main)
pub fn has_fail_policy_errors(config: &Config, briefing: &Briefing) -> bool {
    for source_cfg in &config.sources {
        if source_cfg.on_error == OnError::Fail {
            for section in &briefing.sections {
                for result in &section.sources {
                    if result.id == source_cfg.id && (result.status == "error" || result.status == "timed_out") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn section_title(id: &SectionId) -> String {
    match id {
        SectionId::Health => "Health".to_string(),
        SectionId::Actions => "Pending Actions".to_string(),
        SectionId::Code => "Code".to_string(),
        SectionId::Comms => "Communications".to_string(),
        SectionId::Context => "Context".to_string(),
        SectionId::Ideas => "Ideas & News".to_string(),
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
            cache_ttl_sec: None,
        }
    }

    #[tokio::test]
    async fn test_disabled_sources_filtered() {
        let cfg = make_config(vec![
            shell_source("enabled", vec!["echo", "hi"], true, OnError::Warn),
            shell_source("disabled", vec!["echo", "nope"], false, OnError::Warn),
        ]);
        let briefing = collect(&cfg, "/tmp/test.toml", "explicit").await;
        let all_ids: Vec<_> = briefing
            .sections
            .iter()
            .flat_map(|s| s.sources.iter().map(|r| r.id.as_str()))
            .collect();
        assert!(all_ids.contains(&"enabled"));
        assert!(!all_ids.contains(&"disabled"));
    }

    #[tokio::test]
    async fn test_omit_on_error() {
        let cfg = make_config(vec![shell_source(
            "failing",
            vec!["__no_such_bin_recon__"],
            true,
            OnError::Omit,
        )]);
        let briefing = collect(&cfg, "/tmp/test.toml", "explicit").await;
        assert_eq!(briefing.summary.sources_total, 0);
    }

    #[tokio::test]
    async fn test_warn_on_error_includes_result() {
        let cfg = make_config(vec![shell_source(
            "failing",
            vec!["__no_such_bin_recon__"],
            true,
            OnError::Warn,
        )]);
        let briefing = collect(&cfg, "/tmp/test.toml", "explicit").await;
        let all_ids: Vec<_> = briefing
            .sections
            .iter()
            .flat_map(|s| s.sources.iter().map(|r| r.id.as_str()))
            .collect();
        assert!(all_ids.contains(&"failing"));
        assert!(briefing.partial);
    }

    #[tokio::test]
    async fn test_summary_counts() {
        let cfg = make_config(vec![
            shell_source("ok1", vec!["echo", "hi"], true, OnError::Warn),
            shell_source("fail1", vec!["false"], true, OnError::Warn),
        ]);
        let briefing = collect(&cfg, "/tmp/test.toml", "explicit").await;
        assert_eq!(briefing.summary.sources_ok, 1);
        assert_eq!(briefing.summary.sources_failed, 1);
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
        let briefing = collect(&cfg, "/tmp/test.toml", "explicit").await;
        let ids: Vec<_> = briefing.sections.iter().map(|s| s.id.as_str()).collect();
        let h = ids.iter().position(|&x| x == "health").unwrap();
        let c = ids.iter().position(|&x| x == "code").unwrap();
        let i = ids.iter().position(|&x| x == "ideas").unwrap();
        assert!(h < c);
        assert!(c < i);
    }

    #[tokio::test]
    async fn test_config_populated() {
        let cfg = make_config(vec![]);
        let briefing = collect(&cfg, "/my/config.toml", "global").await;
        assert_eq!(briefing.config.path, "/my/config.toml");
        assert_eq!(briefing.config.scope, "global");
    }

    #[tokio::test]
    async fn test_timeout() {
        let mut src = shell_source("slow", vec!["sleep", "10"], true, OnError::Warn);
        src.timeout_sec = Some(1);
        let cfg = make_config(vec![src]);
        let briefing = collect(&cfg, "/tmp/test.toml", "explicit").await;
        let result = &briefing.sections[0].sources[0];
        // #15: status is "timed_out" not "error"
        assert_eq!(result.status, "timed_out");
        assert_eq!(result.error.as_ref().unwrap().error_type, "timed_out");
        assert!(briefing.partial);
        assert_eq!(briefing.summary.sources_timed_out, 1);
    }

    #[tokio::test]
    async fn test_schema_version() {
        let cfg = make_config(vec![]);
        let briefing = collect(&cfg, "/tmp/test.toml", "explicit").await;
        assert_eq!(briefing.schema_version, "0.1");
    }

    #[tokio::test]
    async fn test_fail_policy() {
        let cfg = make_config(vec![
            shell_source("critical", vec!["false"], true, OnError::Fail),
            shell_source("ok", vec!["echo", "hi"], true, OnError::Warn),
        ]);
        let briefing = collect(&cfg, "/tmp/test.toml", "explicit").await;
        assert!(briefing.partial);
        assert!(has_fail_policy_errors(&cfg, &briefing));
    }
}
