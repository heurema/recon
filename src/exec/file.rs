use std::time::Instant;

use serde_json::Value;
use tokio::fs;

use crate::config::Source;
use crate::exec::shell::{format_to_content_type, parse_output};
use crate::model::{SourceError, SourceResult};

/// Execute a file source: read path and parse by format.
///
/// Paths are already ~-expanded by config.rs before reaching this executor (AC09).
pub async fn execute(source: &Source, max_bytes: usize) -> SourceResult {
    let path = source.path.as_ref().expect("file source must have path");
    let started = Instant::now();

    // Check file size before reading to prevent memory exhaustion
    match fs::metadata(path).await {
        Ok(meta) if meta.len() as usize > max_bytes => {
            let elapsed = started.elapsed().as_millis() as u64;
            return SourceResult::new(
                source.id.clone(),
                "file",
                format_to_content_type(&source.format),
                "error",
                elapsed,
                Value::Null,
                Some(SourceError {
                    error_type: "output_too_large".to_string(),
                    message: format!("file {} is {} bytes, max is {}", path, meta.len(), max_bytes),
                    exit_code: None,
                    stderr: String::new(),
                }),
            );
        }
        Err(_) => {} // will be caught by read below
        _ => {}
    }

    // AC10: file_not_found
    let raw_bytes = match fs::read(path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let elapsed = started.elapsed().as_millis() as u64;
            return SourceResult::new(
                source.id.clone(),
                "file",
                format_to_content_type(&source.format),
                "error",
                elapsed,
                Value::Null,
                Some(SourceError {
                    error_type: "file_not_found".to_string(),
                    message: format!("file not found: {}", path),
                    exit_code: None,
                    stderr: String::new(),
                }),
            );
        }
        Err(e) => {
            let elapsed = started.elapsed().as_millis() as u64;
            // #18: distinguish EISDIR/EACCES from ENOENT
            let error_type = if e.kind() == std::io::ErrorKind::PermissionDenied {
                "parse_error"
            } else if e.raw_os_error() == Some(21) { // EISDIR
                "parse_error"
            } else {
                "file_not_found"
            };
            return SourceResult::new(
                source.id.clone(),
                "file",
                format_to_content_type(&source.format),
                "error",
                elapsed,
                Value::Null,
                Some(SourceError {
                    error_type: error_type.to_string(),
                    message: format!("cannot read {}: {}", path, e),
                    exit_code: None,
                    stderr: String::new(),
                }),
            );
        }
    };

    // AC11: invalid UTF-8 → parse_error
    let content = match String::from_utf8(raw_bytes) {
        Ok(s) => s,
        Err(_) => {
            let elapsed = started.elapsed().as_millis() as u64;
            return SourceResult::new(
                source.id.clone(),
                "file",
                format_to_content_type(&source.format),
                "error",
                elapsed,
                Value::Null,
                Some(SourceError {
                    error_type: "parse_error".to_string(),
                    message: format!("file is not valid UTF-8: {}", path),
                    exit_code: None,
                    stderr: String::new(),
                }),
            );
        }
    };

    let elapsed = started.elapsed().as_millis() as u64;

    // AC12: invalid JSON/JSONL → parse_error with details
    match parse_output(&content, &source.format) {
        Ok(data) => SourceResult::new(
            source.id.clone(),
            "file",
            format_to_content_type(&source.format),
            "ok",
            elapsed,
            data,
            None,
        ),
        Err(msg) => SourceResult::new(
            source.id.clone(),
            "file",
            format_to_content_type(&source.format),
            "error",
            elapsed,
            Value::Null,
            Some(SourceError {
                error_type: "parse_error".to_string(),
                message: format!("{}: {}", path, msg),
                exit_code: None,
                stderr: String::new(),
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{OnError, SectionId, SourceFormat, SourceType};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_file_source(id: &str, path: &str, format: SourceFormat) -> Source {
        Source {
            id: id.to_string(),
            section: SectionId::Context,
            source_type: SourceType::File,
            args: None,
            path: Some(path.to_string()),
            format,
            timeout_sec: None,
            on_error: OnError::Warn,
            enabled: true,
            cache_ttl_sec: None,
        }
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let src = make_file_source(
            "test",
            "/tmp/__recon_no_such_file_xyz__.txt",
            SourceFormat::Text,
        );
        let result = execute(&src, 1024 * 1024).await;
        assert_eq!(result.status, "error");
        assert_eq!(result.error.unwrap().error_type, "file_not_found");
    }

    #[tokio::test]
    async fn test_text_file() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "hello world").unwrap();
        let src = make_file_source("test", f.path().to_str().unwrap(), SourceFormat::Text);
        let result = execute(&src, 1024 * 1024).await;
        assert_eq!(result.status, "ok");
        assert_eq!(result.data, Value::String("hello world".to_string()));
    }

    #[tokio::test]
    async fn test_json_file() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, r#"{{"foo":"bar"}}"#).unwrap();
        let src = make_file_source("test", f.path().to_str().unwrap(), SourceFormat::Json);
        let result = execute(&src, 1024 * 1024).await;
        assert_eq!(result.status, "ok");
        assert_eq!(result.data["foo"], Value::String("bar".to_string()));
    }

    #[tokio::test]
    async fn test_invalid_json_file() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "not json").unwrap();
        let src = make_file_source("test", f.path().to_str().unwrap(), SourceFormat::Json);
        let result = execute(&src, 1024 * 1024).await;
        assert_eq!(result.status, "error");
        assert_eq!(result.error.unwrap().error_type, "parse_error");
    }

    #[tokio::test]
    async fn test_invalid_utf8_file() {
        let path = "/tmp/__recon_test_utf8__.bin";
        std::fs::write(path, b"\xff\xfe invalid utf8 bytes").unwrap();
        let src = make_file_source("test", path, SourceFormat::Text);
        let result = execute(&src, 1024 * 1024).await;
        assert_eq!(result.status, "error");
        assert_eq!(result.error.unwrap().error_type, "parse_error");
        std::fs::remove_file(path).ok();
    }
}
