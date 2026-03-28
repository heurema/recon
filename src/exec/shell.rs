use std::time::Instant;

use serde_json::Value;
#[allow(unused_imports)]
use std::os::unix::process::CommandExt;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::config::{Source, SourceFormat};
use crate::model::{SourceError, SourceResult};

/// Spawn a shell source and return (Child, pgid) for external timeout management.
pub fn spawn_child(source: &Source) -> Result<(tokio::process::Child, Option<i32>), SourceResult> {
    let args = source.args.as_ref().expect("shell source must have args");
    let binary = &args[0];

    if which::which(binary).is_err() {
        return Err(SourceResult::new(
            source.id.clone(),
            "shell",
            format_to_content_type(&source.format),
            "error",
            0,
            Value::Null,
            Some(SourceError {
                error_type: "command_not_found".to_string(),
                message: format!("command not found: {}", binary),
                exit_code: None,
                stderr: String::new(),
            }),
        ));
    }

    let mut cmd = Command::new(binary);
    if args.len() > 1 {
        cmd.args(&args[1..]);
    }
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    // #3: clean environment — only pass safe vars
    cmd.env_clear();
    for var in &["PATH", "HOME", "USER", "LANG", "TMPDIR", "TZ", "TERM"] {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }

    unsafe {
        cmd.pre_exec(|| {
            libc::setpgid(0, 0);
            Ok(())
        });
    }

    match cmd.spawn() {
        Ok(child) => {
            // #7: use Option to prevent killpg(0) self-kill
            let pgid = child.id().map(|id| id as i32);
            Ok((child, pgid))
        }
        Err(e) => Err(SourceResult::new(
            source.id.clone(),
            "shell",
            format_to_content_type(&source.format),
            "error",
            0,
            Value::Null,
            Some(SourceError {
                error_type: "command_not_found".to_string(),
                message: format!("failed to spawn {}: {}", binary, e),
                exit_code: None,
                stderr: String::new(),
            }),
        )),
    }
}

/// Execute an already-spawned child process with streaming stdout cap.
/// #2: reads stdout incrementally up to max_output_bytes, then kills.
pub async fn execute_child(
    mut child: tokio::process::Child,
    source: &Source,
    max_output_bytes: usize,
) -> SourceResult {
    let started = Instant::now();
    let binary = source
        .args
        .as_ref()
        .map(|a| a[0].as_str())
        .unwrap_or("unknown");

    // Stream stdout with size cap instead of wait_with_output
    let mut stdout_buf = Vec::with_capacity(max_output_bytes.min(65536));
    let mut stderr_buf = Vec::with_capacity(4096);
    let mut stdout_exceeded = false;

    let mut child_stdout = child.stdout.take();
    let mut child_stderr = child.stderr.take();

    // Read stdout up to limit
    if let Some(ref mut stdout) = child_stdout {
        let mut buf = [0u8; 8192];
        loop {
            match stdout.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let remaining = max_output_bytes.saturating_sub(stdout_buf.len());
                    if remaining == 0 {
                        stdout_exceeded = true;
                        break;
                    }
                    let take = n.min(remaining);
                    stdout_buf.extend_from_slice(&buf[..take]);
                    if take < n {
                        stdout_exceeded = true;
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    }

    // Read stderr (capped at 4KB)
    if let Some(ref mut stderr) = child_stderr {
        let mut buf = [0u8; 4096];
        if let Ok(n) = stderr.read(&mut buf).await {
            stderr_buf.extend_from_slice(&buf[..n]);
        }
    }

    // Drop handles so child can exit
    drop(child_stdout);
    drop(child_stderr);

    // If output exceeded, kill the child
    if stdout_exceeded {
        if let Some(pgid) = child.id().map(|id| id as i32) {
            kill_process_group_sync(pgid);
        }
        let _ = child.wait().await;
        let elapsed = started.elapsed().as_millis() as u64;
        return SourceResult::new(
            source.id.clone(),
            "shell",
            format_to_content_type(&source.format),
            "error",
            elapsed,
            Value::Null,
            Some(SourceError {
                error_type: "output_too_large".to_string(),
                message: format!(
                    "stdout exceeded {} bytes, process killed",
                    max_output_bytes
                ),
                exit_code: None,
                stderr: SourceError::sanitized_stderr(
                    &String::from_utf8_lossy(&stderr_buf),
                ),
            }),
        );
    }

    let status = match child.wait().await {
        Ok(s) => s,
        Err(e) => {
            let elapsed = started.elapsed().as_millis() as u64;
            return SourceResult::new(
                source.id.clone(),
                "shell",
                format_to_content_type(&source.format),
                "error",
                elapsed,
                Value::Null,
                Some(SourceError {
                    error_type: "command_failed".to_string(),
                    message: format!("I/O error waiting for {}: {}", binary, e),
                    exit_code: None,
                    stderr: String::new(),
                }),
            );
        }
    };

    let elapsed = started.elapsed().as_millis() as u64;
    let stderr_raw = String::from_utf8_lossy(&stderr_buf).into_owned();
    let sanitized = SourceError::sanitized_stderr(&stderr_raw);

    if !status.success() {
        let exit_code = status.code();
        return SourceResult::new(
            source.id.clone(),
            "shell",
            format_to_content_type(&source.format),
            "error",
            elapsed,
            Value::Null,
            Some(SourceError {
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
        );
    }

    let stdout_str = match std::str::from_utf8(&stdout_buf) {
        Ok(s) => s,
        Err(_) => {
            return SourceResult::new(
                source.id.clone(),
                "shell",
                format_to_content_type(&source.format),
                "error",
                elapsed,
                Value::Null,
                Some(SourceError {
                    error_type: "parse_error".to_string(),
                    message: "stdout is not valid UTF-8".to_string(),
                    exit_code: None,
                    stderr: sanitized,
                }),
            );
        }
    };

    match parse_output(stdout_str, &source.format) {
        Ok(data) => SourceResult::new(
            source.id.clone(),
            "shell",
            format_to_content_type(&source.format),
            "ok",
            elapsed,
            data,
            None,
        ),
        Err(msg) => SourceResult::new(
            source.id.clone(),
            "shell",
            format_to_content_type(&source.format),
            "error",
            elapsed,
            Value::Null,
            Some(SourceError {
                error_type: "parse_error".to_string(),
                message: msg,
                exit_code: Some(0),
                stderr: sanitized,
            }),
        ),
    }
}

/// Convenience wrapper for tests: spawns + waits.
pub async fn execute(source: &Source, max_output_bytes: usize) -> SourceResult {
    match spawn_child(source) {
        Err(err_result) => err_result,
        Ok((child, _pgid)) => execute_child(child, source, max_output_bytes).await,
    }
}

/// #7: kill entire process group, guarding against pgid=0
pub async fn kill_process_group(pgid: Option<i32>) {
    let Some(pgid) = pgid else { return };
    if pgid <= 0 {
        return;
    }
    kill_process_group_sync(pgid);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    unsafe {
        libc::killpg(pgid, libc::SIGKILL);
    }
}

fn kill_process_group_sync(pgid: i32) {
    if pgid <= 0 {
        return;
    }
    unsafe {
        libc::killpg(pgid, libc::SIGTERM);
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
            cache_ttl_sec: None,
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
    }

    #[tokio::test]
    async fn test_max_output_bytes_cap() {
        let src = make_source(
            "test",
            vec!["sh", "-c", "dd if=/dev/zero bs=1024 count=10 2>/dev/null | tr '\\0' 'x'"],
            SourceFormat::Text,
        );
        let result = execute(&src, 100).await;
        // Should either cap or report output_too_large
        if result.status == "ok" {
            if let Value::String(s) = &result.data {
                assert!(s.len() <= 100);
            }
        } else {
            let err = result.error.as_ref().unwrap();
            assert!(
                err.error_type == "output_too_large" || err.error_type == "parse_error",
                "unexpected error type: {}",
                err.error_type
            );
        }
    }

    #[tokio::test]
    async fn test_env_not_inherited() {
        unsafe { std::env::set_var("RECON_TEST_SECRET", "should_not_leak") };
        let src = make_source("test", vec!["sh", "-c", "echo $RECON_TEST_SECRET"], SourceFormat::Text);
        let result = execute(&src, 1024 * 1024).await;
        assert_eq!(result.status, "ok");
        if let Value::String(s) = &result.data {
            assert!(!s.contains("should_not_leak"), "env var leaked: {}", s);
        }
        unsafe { std::env::remove_var("RECON_TEST_SECRET") };
    }
}
