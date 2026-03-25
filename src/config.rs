use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppError;

type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Defaults {
    pub timeout_sec: u64,
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum SectionId {
    Health,   // 0 — first in output
    Actions,  // 1
    Code,     // 2
    Comms,    // 3
    Context,  // 4
    Ideas,    // 5 — last in output
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    #[serde(rename = "shell")]
    Shell,
    #[serde(rename = "file")]
    File,
}

impl Default for SourceType {
    fn default() -> Self {
        SourceType::Shell
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceFormat {
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "jsonl")]
    Jsonl,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "markdown")]
    Markdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OnError {
    #[serde(rename = "warn")]
    Warn,
    #[serde(rename = "fail")]
    Fail,
    #[serde(rename = "omit")]
    Omit,
}

impl Default for OnError {
    fn default() -> Self {
        OnError::Warn
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub section: SectionId,
    #[serde(rename = "type", default)]
    pub source_type: SourceType,
    pub args: Option<Vec<String>>,
    pub path: Option<String>,
    pub format: SourceFormat,
    pub timeout_sec: Option<u64>,
    #[serde(default)]
    pub on_error: OnError,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema_version: u32,
    pub defaults: Defaults,
    #[serde(alias = "source")]
    pub sources: Vec<Source>,
}

impl Config {
    pub fn load(path: Option<&Path>) -> Result<Config> {
        let config_path = Self::resolve_path(path)?;
        let content = std::fs::read_to_string(&config_path)?;
        let mut config: Config = toml::from_str(&content)
            .map_err(|e| AppError::ParseError(format!("invalid TOML in {}: {}", config_path.display(), e)))?;

        config.validate()?;
        config.expand_paths();

        Ok(config)
    }

    fn resolve_path(explicit: Option<&Path>) -> Result<PathBuf> {
        // 1. explicit --config path (highest priority — user intent)
        if let Some(p) = explicit {
            return Ok(p.to_path_buf());
        }

        // 2. RECON_CONFIG env var (lower than explicit to prevent env substitution attacks)
        if let Ok(env_path) = std::env::var("RECON_CONFIG") {
            if !env_path.is_empty() {
                return Ok(PathBuf::from(env_path));
            }
        }

        // 3. ~/.config/recon/briefing.toml
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::ConfigError("cannot determine home directory".to_string()))?;
        let default_path = home.join(".config").join("recon").join("briefing.toml");

        if default_path.exists() {
            return Ok(default_path);
        }

        Err(AppError::ConfigError(
            "no config found: set RECON_CONFIG, pass --config, or create ~/.config/recon/briefing.toml".to_string(),
        ))
    }

    fn validate(&self) -> Result<()> {
        // schema_version must be 1
        if self.schema_version != 1 {
            return Err(AppError::ValidationError(format!(
                "schema_version must be 1, got {}",
                self.schema_version
            )));
        }

        // timeout_sec in defaults must not be 0
        if self.defaults.timeout_sec == 0 {
            return Err(AppError::ValidationError(
                "defaults.timeout_sec must be greater than 0".to_string(),
            ));
        }

        // #19: max_output_bytes must be > 0
        if self.defaults.max_output_bytes == 0 {
            return Err(AppError::ValidationError(
                "defaults.max_output_bytes must be greater than 0".to_string(),
            ));
        }

        // Check for duplicate source IDs
        let mut seen_ids: HashSet<&str> = HashSet::new();
        for source in &self.sources {
            if !seen_ids.insert(source.id.as_str()) {
                return Err(AppError::ValidationError(format!(
                    "duplicate source id: {}",
                    source.id
                )));
            }
        }

        // Validate individual sources
        for source in &self.sources {
            self.validate_source(source)?;
        }

        Ok(())
    }

    fn validate_source(&self, source: &Source) -> Result<()> {
        match source.source_type {
            SourceType::Shell => {
                // args must be present and non-empty
                match &source.args {
                    None => {
                        return Err(AppError::ValidationError(format!(
                            "source '{}': shell type requires args",
                            source.id
                        )));
                    }
                    Some(args) if args.is_empty() => {
                        return Err(AppError::ValidationError(format!(
                            "source '{}': shell type args must not be empty",
                            source.id
                        )));
                    }
                    _ => {}
                }
                // path must be absent
                if source.path.is_some() {
                    return Err(AppError::ValidationError(format!(
                        "source '{}': shell type must not have path",
                        source.id
                    )));
                }
                // validate per-source timeout_sec != 0
                if let Some(t) = source.timeout_sec {
                    if t == 0 {
                        return Err(AppError::ValidationError(format!(
                            "source '{}': timeout_sec must be greater than 0",
                            source.id
                        )));
                    }
                }
            }
            SourceType::File => {
                // path must be present
                if source.path.is_none() {
                    return Err(AppError::ValidationError(format!(
                        "source '{}': file type requires path",
                        source.id
                    )));
                }
                // args must be absent
                if source.args.is_some() {
                    return Err(AppError::ValidationError(format!(
                        "source '{}': file type must not have args",
                        source.id
                    )));
                }
                // timeout_sec on file type is silently ignored (no validation)
            }
        }

        Ok(())
    }

    /// Expand ~/... in Source.path fields only. args fields are not expanded.
    /// Only expands `~/` prefix (home-relative). `~user/` patterns are not supported.
    fn expand_paths(&mut self) {
        let home = dirs::home_dir();
        for source in &mut self.sources {
            if let Some(ref p) = source.path {
                if p.starts_with("~/") || p == "~" {
                    if let Some(ref home_dir) = home {
                        let rest = if p.len() > 2 { &p[2..] } else { "" };
                        let expanded = home_dir.join(rest);
                        source.path = Some(expanded.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_from_str(toml: &str) -> std::result::Result<Config, AppError> {
        let mut config: Config = toml::from_str(toml)
            .map_err(|e| AppError::ParseError(format!("invalid TOML: {}", e)))?;
        config.validate()?;
        config.expand_paths();
        Ok(config)
    }

    #[test]
    fn test_valid_config_load() {
        let toml = r#"
schema_version = 1

[defaults]
timeout_sec = 10
max_output_bytes = 102400

[[sources]]
id = "git-status"
section = "code"
type = "shell"
args = ["git", "status", "--short"]
format = "text"
"#;
        let config = load_from_str(toml).expect("valid config should parse");
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.sources.len(), 1);
        assert_eq!(config.sources[0].id, "git-status");
    }

    #[test]
    fn test_duplicate_id_rejection() {
        let toml = r#"
schema_version = 1

[defaults]
timeout_sec = 10
max_output_bytes = 102400

[[sources]]
id = "git-status"
section = "code"
type = "shell"
args = ["git", "status"]
format = "text"

[[sources]]
id = "git-status"
section = "health"
type = "shell"
args = ["git", "log", "--oneline", "-5"]
format = "text"
"#;
        let err = load_from_str(toml).expect_err("duplicate IDs should fail");
        let msg = err.to_string();
        assert!(msg.contains("duplicate"), "error should mention 'duplicate': {}", msg);
        assert!(msg.contains("git-status"), "error should mention the duplicate id: {}", msg);
    }

    #[test]
    fn test_empty_args_rejection_for_shell() {
        let toml = r#"
schema_version = 1

[defaults]
timeout_sec = 10
max_output_bytes = 102400

[[sources]]
id = "bad-source"
section = "code"
type = "shell"
args = []
format = "text"
"#;
        let err = load_from_str(toml).expect_err("empty args should fail");
        let msg = err.to_string();
        assert!(msg.contains("empty") || msg.contains("args"), "error should mention args: {}", msg);
    }

    #[test]
    fn test_schema_version_validation() {
        let toml = r#"
schema_version = 2

[defaults]
timeout_sec = 10
max_output_bytes = 102400

[[sources]]
id = "git-status"
section = "code"
type = "shell"
args = ["git", "status"]
format = "text"
"#;
        let err = load_from_str(toml).expect_err("wrong schema_version should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("schema_version") || msg.contains("version"),
            "error should mention schema_version: {}",
            msg
        );
    }

    #[test]
    fn test_tilde_expansion_in_path() {
        let home = dirs::home_dir().expect("home dir must exist in test env");
        let toml = r#"
schema_version = 1

[defaults]
timeout_sec = 10
max_output_bytes = 102400

[[sources]]
id = "my-file"
section = "context"
type = "file"
path = "~/notes/context.md"
format = "markdown"
"#;
        let config = load_from_str(toml).expect("valid config with ~ path should parse");
        let expanded = config.sources[0].path.as_ref().expect("path should be set");
        assert!(
            expanded.starts_with(home.to_str().unwrap()),
            "~ should expand to home dir, got: {}",
            expanded
        );
        assert!(!expanded.starts_with('~'), "path should not start with ~ after expansion");
    }

    #[test]
    fn test_timeout_zero_rejection() {
        let toml = r#"
schema_version = 1

[defaults]
timeout_sec = 0
max_output_bytes = 102400

[[sources]]
id = "git-status"
section = "code"
type = "shell"
args = ["git", "status"]
format = "text"
"#;
        let err = load_from_str(toml).expect_err("timeout_sec=0 should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("timeout") || msg.contains("0"),
            "error should mention timeout: {}",
            msg
        );
    }

    #[test]
    fn test_file_type_requires_path() {
        let toml = r#"
schema_version = 1

[defaults]
timeout_sec = 10
max_output_bytes = 102400

[[sources]]
id = "my-file"
section = "context"
type = "file"
format = "text"
"#;
        let err = load_from_str(toml).expect_err("file type without path should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("path") || msg.contains("file"),
            "error should mention path: {}",
            msg
        );
    }

    #[test]
    fn test_file_type_ignores_timeout_sec() {
        let toml = r#"
schema_version = 1

[defaults]
timeout_sec = 10
max_output_bytes = 102400

[[sources]]
id = "my-file"
section = "context"
type = "file"
path = "/tmp/test.md"
format = "markdown"
timeout_sec = 5
"#;
        // Should not error even though timeout_sec is set on a file type
        let config = load_from_str(toml).expect("file type with timeout_sec should be silently ignored");
        assert_eq!(config.sources[0].timeout_sec, Some(5));
    }
}
