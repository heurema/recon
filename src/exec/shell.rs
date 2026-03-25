use std::time::Instant;

use serde_json::Value;
#[allow(unused_imports)]
use std::os::unix::process::CommandExt;
use tokio::process::Command;

use crate::config::{Source, SourceFormat};
use crate::model::{SourceError, SourceResult};

/// Spawn a shell source and return (Child, pid) for external timeout management.
/// Returns None if binary not found or spawn fails.
pub fn spawn_child(source: &Source) -> Result<(tokio::process::Child, i32), SourceResult> {
    let args = source.args.as_ref().expect("shell source must have args");
    let binary = &args[0];

    if which::which(binary).is_err() {
        return Err(SourceResult {
            id: source.id.clone(),
            source_type: "shell".to_string(),
            content_type: format_to_content_type(&source.format),
            trust: "untrusted".to_string(),
            status: "error".to_string(),
            duration_ms: 0,
            data: Value::Null,
            error: Some(SourceError {
                error_type: "command_not_found".to_string(),
                message: format!("command not found: {}", binary),
                exit_code: None,
                stderr: String::new(),
            }),
        });
    }

    let mut cmd = Command::new(binary);
    if args.len() > 1 {
        cmd.args(&args[1..]);
    }
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    unsafe {
        cmd.pre_exec(|| {
            libc::setpgid(0, 0);
            Ok(())
        });
    }

    match cmd.spawn() {
        Ok(child) => {
            let pid = child.id().unwrap_or(0) as i32;
            Ok((child, pid))
        }
        Err(e) => Err(SourceResult {
            id: source.id.clone(),
            source_type: "shell".to_string(),
            content_type: format_to_content_type(&source.format),
            trust: "untrusted".to_string(),
            status: "error".to_string(),
            duration_ms: 0,
            data: Value::Null,
            error: Some(SourceError {
                error_type: "command_not_found".to_string(),
                message: format!("failed to spawn {}: {}", binary, e),
                exit_code: None,
                stderr: String::new(),
            }),
        }),
    }
}

/// Execute an already-spawned child process. Called by runner after spawn_child.
pub async fn execute_child(child: tokio::process::Child, source: &Source, max_output_bytes: usize) -> SourceResult {
    let started = Instant::now();
    let binary = source.args.as_ref().map(|a| a[0].as_str()).unwrap_or("unknown");

    let output = match child.wait_with_output().await {
        Ok(o) => o,
        Err(e) => {
            return SourceResult {
                id: source.id.clone(),
                source_type: "shell".to_string(),
                content_type: format_to_content_type(&source.format),
                trust: "untrusted".to_string(),
                status: "error".to_string(),
                duration_ms: started.elapsed().as_millis() as u64,
                data: Value::Null,
                error: Some(SourceError {
                    error_type: "command_failed".to_string(),
                    message: format!("I/O error waiting for {}: {}", binary, e),
                    exit_code: None,
                    stderr: String::new(),
                }),
            };
        }
    };

    let elapsed = started.elapsed().as_millis() as u64;
    let stderr_raw = String::from_utf8_lossy(&output.stderr).into_owned();
    let sanitized = SourceError::sanitized_stderr(&stderr_raw);

    if !output.status.success() {
        return SourceResult {
            id: source.id.clone(),
            source_type: "shell".to_string(),
            content_type: format_to_content_type(&source.format),
            trust: "untrusted".to_string(),
            status: "error".to_string(),
            duration_ms: elapsed,
            data: Value::Null,
            error: Some(SourceError {
                error_type: "command_failed".to_string(),
                message: format!("command exited with code {}", output.status.code().map(|c| c.to_string()).unwrap_or("unknown".into())),
                exit_code: output.status.code(),
                stderr: sanitized,
            }),
        };
    }

    let stdout_bytes = if output.stdout.len() > max_output_bytes {
        &output.stdout[..max_output_bytes]
    } else {
        &output.stdout
    };

    let stdout_str = match std::str::from_utf8(stdout_bytes) {
        Ok(s) => s,
        Err(_) => {
            return SourceResult {
                id: source.id.clone(),
                source_type: "shell".to_string(),
                content_type: format_to_content_type(&source.format),
                trust: "untrusted".to_string(),
                status: "error".to_string(),
                duration_ms: elapsed,
                data: Value::Null,
                error: Some(SourceError {
                    error_type: "parse_error".to_string(),
                    message: "stdout is not valid UTF-8".to_string(),
                    exit_code: None,
                    stderr: sanitized,
                }),
            };
        }
    };

    match parse_output(stdout_str, &source.format) {
        Ok(data) => SourceResult {
            id: source.id.clone(),
            source_type: "shell".to_string(),
            content_type: format_to_content_type(&source.format),
            trust: "untrusted".to_string(),
            status: "ok".to_string(),
            duration_ms: elapsed,
            data,
            error: None,
        },
        Err(msg) => SourceResult {
            id: source.id.clone(),
            source_type: "shell".to_string(),
            content_type: format_to_content_type(&source.format),
            trust: "untrusted".to_string(),
            status: "error".to_string(),
            duration_ms: elapsed,
            data: Value::Null,
            error: Some(SourceError {
                error_type: "parse_error".to_string(),
                message: msg,
                exit_code: Some(0),
                stderr: sanitized,
            }),
        },
    }
}

/// Execute a shell source (args-based, no shell interpolation).
/// Convenience wrapper: spawns + waits. Used by tests.
pub async fn execute(source: &Source, max_output_bytes: usize) -> SourceResult {
    let args = source.args.as_ref().expect("shell source must have args");
    let binary = &args[0];

    let started = Instant::now();

    // AC05: check binary exists before spawning
    if which::which(binary).is_err() {
        let elapsed = started.elapsed().as_millis() as u64;
        return SourceResult {
            id: source.id.clone(),
            source_type: "shell".to_string(),
            content_type: format_to_content_type(&source.format),
            trust: "untrusted".to_string(),
            status: "error".to_string(),
            duration_ms: elapsed,
            data: Value::Null,
            error: Some(SourceError {
                error_type: "command_not_found".to_string(),
                message: format!("command not found: {}", binary),
                exit_code: None,
                stderr: String::new(),
            }),
        };
    }

    // AC01 + AC02: spawn with process group isolation (no shell interpretation)
    let mut cmd = Command::new(binary);
    if args.len() > 1 {
        cmd.args(&args[1..]);
    }
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    // process_group(0): child becomes its own process group leader (setsid equivalent)
    unsafe {
        cmd.pre_exec(|| {
            libc::setpgid(0, 0);
            Ok(())
        });
    }

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let elapsed = started.elapsed().as_millis() as u64;
            return SourceResult {
                id: source.id.clone(),
                source_type: "shell".to_string(),
                content_type: format_to_content_type(&source.format),
                trust: "untrusted".to_string(),
                status: "error".to_string(),
                duration_ms: elapsed,
                data: Value::Null,
                error: Some(SourceError {
                    error_type: "command_not_found".to_string(),
                    message: format!("failed to spawn {}: {}", binary, e),
                    exit_code: None,
                    stderr: String::new(),
                }),
            };
        }
    };

    let pid = child.id().unwrap_or(0) as i32;

    let output = match child.wait_with_output().await {
        Ok(o) => o,
        Err(e) => {
            let elapsed = started.elapsed().as_millis() as u64;
            return SourceResult {
                id: source.id.clone(),
                source_type: "shell".to_string(),
                content_type: format_to_content_type(&source.format),
                trust: "untrusted".to_string(),
                status: "error".to_string(),
                duration_ms: elapsed,
                data: Value::Null,
                error: Some(SourceError {
                    error_type: "command_failed".to_string(),
                    message: format!("I/O error waiting for process: {}", e),
                    exit_code: None,
                    stderr: String::new(),
                }),
            };
        }
    };

    let elapsed = started.elapsed().as_millis() as u64;
    let _ = pid;

    // AC25: all stderr goes through sanitized_stderr()
    let stderr_raw = String::from_utf8_lossy(&output.stderr).into_owned();
    let sanitized = SourceError::sanitized_stderr(&stderr_raw);

    // AC07: non-zero exit code → command_failed
    if !output.status.success() {
        let exit_code = output.status.code();
        return SourceResult {
            id: source.id.clone(),
            source_type: "shell".to_string(),
            content_type: format_to_content_type(&source.format),
            trust: "untrusted".to_string(),
            status: "error".to_string(),
            duration_ms: elapsed,
            data: Value::Null,
            error: Some(SourceError {
                error_type: "command_failed".to_string(),
                message: format!(
                    "command exited with code {}",
                    exit_code
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                ),
                exit_code,
                stderr: sanitized,
            }),
        };
    }

    // AC04: cap stdout at max_output_bytes
    let stdout_bytes = if output.stdout.len() > max_output_bytes {
        &output.stdout[..max_output_bytes]
    } else {
        &output.stdout
    };

    let stdout_str = match std::str::from_utf8(stdout_bytes) {
        Ok(s) => s,
        Err(_) => {
            return SourceResult {
                id: source.id.clone(),
                source_type: "shell".to_string(),
                content_type: format_to_content_type(&source.format),
                trust: "untrusted".to_string(),
                status: "error".to_string(),
                duration_ms: elapsed,
                data: Value::Null,
                error: Some(SourceError {
                    error_type: "parse_error".to_string(),
                    message: "stdout is not valid UTF-8".to_string(),
                    exit_code: None,
                    stderr: sanitized,
                }),
            };
        }
    };

    // AC06: parse by format
    match parse_output(stdout_str, &source.format) {
        Ok(data) => SourceResult {
            id: source.id.clone(),
            source_type: "shell".to_string(),
            content_type: format_to_content_type(&source.format),
            trust: "untrusted".to_string(),
            status: "ok".to_string(),
            duration_ms: elapsed,
            data,
            error: None,
        },
        Err(msg) => SourceResult {
            id: source.id.clone(),
            source_type: "shell".to_string(),
            content_type: format_to_content_type(&source.format),
            trust: "untrusted".to_string(),
            status: "error".to_string(),
            duration_ms: elapsed,
            data: Value::Null,
            error: Some(SourceError {
                error_type: "parse_error".to_string(),
                message: msg,
                exit_code: Some(0),
                stderr: sanitized,
            }),
        },
    }
}

/// AC03: kill entire process group with cascading SIGTERM → wait 2s → SIGKILL
pub async fn kill_process_group(pgid: i32) {
    unsafe {
        libc::killpg(pgid, libc::SIGTERM);
    }
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    unsafe {
        libc::killpg(pgid, libc::SIGKILL);
    }
}

pub fn format_to_content_type(format: &SourceFormat) -> String {
    match format {
        SourceFormat::Json => "json".to_string(),
        SourceFormat::Jsonl => "jsonl".to_string(),
        SourceFormat::Text => "text".to_string(),
        SourceFormat::Markdown => "markdown".to_string(),
    }
}

pub fn parse_output(stdout: &str, format: &SourceFormat) -> Result<Value, String> {
    match format {
        SourceFormat::Json => {
            serde_json::from_str(stdout).map_err(|e| format!("invalid JSON: {}", e))
        }
        SourceFormat::Jsonl => {
            let values: Result<Vec<Value>, _> = stdout
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| serde_json::from_str(l))
                .collect();
            values
                .map(Value::Array)
                .map_err(|e| format!("invalid JSONL: {}", e))
        }
        SourceFormat::Text | SourceFormat::Markdown => Ok(Value::String(stdout.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{OnError, SectionId, SourceType};

    fn make_source(id: &str, args: Vec<&str>, format: SourceFormat) -> Source {
        Source {
            id: id.to_string(),
            section: SectionId::Code,
            source_type: SourceType::Shell,
            args: Some(args.into_iter().map(String::from).collect()),
            path: None,
            format,
            timeout_sec: None,
            on_error: OnError::Warn,
            enabled: true,
        }
    }

    #[tokio::test]
    async fn test_command_not_found() {
        let src = make_source(
            "test",
            vec!["__no_such_binary_recon_test__"],
            SourceFormat::Text,
        );
        let result = execute(&src, 1024 * 1024).await;
        assert_eq!(result.status, "error");
        assert_eq!(result.error.unwrap().error_type, "command_not_found");
    }

    #[tokio::test]
    async fn test_success_text() {
        let src = make_source("test", vec!["echo", "hello"], SourceFormat::Text);
        let result = execute(&src, 1024 * 1024).await;
        assert_eq!(result.status, "ok");
        assert_eq!(result.data, Value::String("hello\n".to_string()));
        assert_eq!(result.trust, "untrusted");
    }

    #[tokio::test]
    async fn test_command_failed() {
        let src = make_source("test", vec!["false"], SourceFormat::Text);
        let result = execute(&src, 1024 * 1024).await;
        assert_eq!(result.status, "error");
        let err = result.error.unwrap();
        assert_eq!(err.error_type, "command_failed");
        assert!(err.exit_code.is_some());
    }

    #[tokio::test]
    async fn test_json_parse() {
        let src = make_source(
            "test",
            vec!["echo", r#"{"key":"value"}"#],
            SourceFormat::Json,
        );
        let result = execute(&src, 1024 * 1024).await;
        assert_eq!(result.status, "ok");
        assert_eq!(result.data["key"], Value::String("value".to_string()));
    }

    #[tokio::test]
    async fn test_jsonl_parse() {
        let src = make_source(
            "test",
            vec!["sh", "-c", r#"printf '{"a":1}\n{"b":2}\n'"#],
            SourceFormat::Jsonl,
        );
        let result = execute(&src, 1024 * 1024).await;
        assert_eq!(result.status, "ok");
        assert!(result.data.is_array());
        let arr = result.data.as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[tokio::test]
    async fn test_max_output_bytes_cap() {
        let src = make_source(
            "test",
            vec!["sh", "-c", "printf '%0.s.' {1..100}"],
            SourceFormat::Text,
        );
        let result = execute(&src, 10).await;
        if let Value::String(s) = &result.data {
            assert!(s.len() <= 10, "stdout not capped: {} bytes", s.len());
        }
    }
}
