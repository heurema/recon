use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingConfig {
    pub path: String,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingSummary {
    pub sources_total: usize,
    pub sources_ok: usize,
    pub sources_failed: usize,
    pub sources_timed_out: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Briefing {
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub partial: bool,
    pub config: BriefingConfig,
    pub summary: BriefingSummary,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub sources: Vec<SourceResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceResult {
    pub id: String,
    #[serde(rename = "type")]
    pub source_type: String,
    pub content_type: String,
    pub trust: String,
    pub status: String,
    pub duration_ms: u64,
    pub data: serde_json::Value,
    pub error: Option<SourceError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
    pub exit_code: Option<i32>,
    /// Truncated to 1KB. Sensitive patterns (tokens, keys) are redacted.
    pub stderr: String,
}

impl SourceError {
    /// Truncate stderr to 1KB and redact potential secrets.
    pub fn sanitized_stderr(raw: &str) -> String {
        let truncated = if raw.len() > 1024 {
            format!("{}... [truncated]", &raw[..1024])
        } else {
            raw.to_string()
        };
        let patterns = ["token=", "key=", "password=", "secret=", "authorization:"];
        let mut result = truncated;
        for pat in &patterns {
            // Redact ALL occurrences (case-insensitive)
            loop {
                let lower = result.to_lowercase();
                let Some(pos) = lower.find(pat) else { break };
                let end = result[pos + pat.len()..]
                    .find(|c: char| c.is_whitespace() || c == '&' || c == '"' || c == '\'' || c == '\n')
                    .map(|i| pos + pat.len() + i)
                    .unwrap_or(result.len());
                result.replace_range(pos..end, "[REDACTED]");
            }
        }
        result
    }
}
