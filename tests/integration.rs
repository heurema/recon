use std::io::Write;
use std::process::Command;

use tempfile::NamedTempFile;

// Path to the release binary (built before tests run)
fn recon_bin() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    format!("{}/target/release/recon", manifest)
}

/// Write a temporary TOML config and return the file handle (keeps file alive).
fn write_config(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().expect("tempfile");
    f.write_all(content.as_bytes()).expect("write config");
    f
}

fn minimal_config(sources: &str) -> String {
    format!(
        "schema_version = 1\n[defaults]\ntimeout_sec = 5\nmax_output_bytes = 102400\n{}",
        sources
    )
}

// ── AC01 / AC02 — recon run produces valid JSON ───────────────────────────

#[test]
fn test_run_json_schema_version() {
    let cfg = write_config(&minimal_config(
        "[[sources]]\nid = \"echo1\"\nsection = \"health\"\ntype = \"shell\"\nargs = [\"echo\", \"hello\"]\nformat = \"text\"\n",
    ));
    let out = Command::new(recon_bin())
        .args(["run", "--config", cfg.path().to_str().unwrap()])
        .output()
        .expect("run recon");

    assert_eq!(out.status.code(), Some(0), "exit 0 on success");

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout must be valid JSON");

    assert_eq!(json["schema_version"], "1", "schema_version must be '1'");
    assert_eq!(json["sections"][0]["id"], "health");
}

#[test]
fn test_run_trust_untrusted() {
    let cfg = write_config(&minimal_config(
        "[[sources]]\nid = \"s1\"\nsection = \"health\"\ntype = \"shell\"\nargs = [\"echo\", \"data\"]\nformat = \"text\"\n",
    ));
    let out = Command::new(recon_bin())
        .args(["run", "--config", cfg.path().to_str().unwrap()])
        .output()
        .expect("run recon");

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("valid JSON");

    assert_eq!(
        json["sections"][0]["sources"][0]["trust"],
        "untrusted",
        "trust must be 'untrusted'"
    );
}

// ── AC03 — text format uses external_data tags ────────────────────────────

#[test]
fn test_run_text_format_external_data_tag() {
    let cfg = write_config(&minimal_config(
        "[[sources]]\nid = \"test-src\"\nsection = \"code\"\ntype = \"shell\"\nargs = [\"echo\", \"test\"]\nformat = \"text\"\n",
    ));
    let out = Command::new(recon_bin())
        .args(["run", "--config", cfg.path().to_str().unwrap(), "--format", "text"])
        .output()
        .expect("run recon");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("<external_data"),
        "stdout must contain <external_data tag, got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("trust=\"untrusted\""),
        "external_data must have trust attribute, got:\n{}",
        stdout
    );
}

// ── AC06 — exit code 2 for missing config ────────────────────────────────

#[test]
fn test_exit_2_missing_config() {
    let out = Command::new(recon_bin())
        .args(["run", "--config", "/nonexistent/path/config.toml"])
        .output()
        .expect("run recon");

    assert_eq!(out.status.code(), Some(2), "missing config must exit 2");
}

// ── AC07 — verbose writes to stderr, stdout stays JSON ───────────────────

#[test]
fn test_verbose_stderr_only() {
    let cfg = write_config(&minimal_config(
        "[[sources]]\nid = \"s\"\nsection = \"health\"\ntype = \"shell\"\nargs = [\"echo\", \"x\"]\nformat = \"text\"\n",
    ));
    let out = Command::new(recon_bin())
        .args(["run", "--verbose", "--config", cfg.path().to_str().unwrap()])
        .output()
        .expect("run recon");

    // stdout must still be valid JSON
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout must be valid JSON even with --verbose");

    assert!(json.is_object(), "stdout must be JSON object");
    // stderr should have verbose output
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.is_empty(),
        "stderr should contain verbose diagnostics"
    );
}

// ── AC08 — --section filter ───────────────────────────────────────────────

#[test]
fn test_section_filter() {
    let cfg = write_config(&minimal_config(concat!(
        "[[sources]]\nid = \"h1\"\nsection = \"health\"\ntype = \"shell\"\nargs = [\"echo\", \"1\"]\nformat = \"text\"\n",
        "[[sources]]\nid = \"c1\"\nsection = \"code\"\ntype = \"shell\"\nargs = [\"echo\", \"2\"]\nformat = \"text\"\n",
    )));
    let out = Command::new(recon_bin())
        .args([
            "run",
            "--config",
            cfg.path().to_str().unwrap(),
            "--section",
            "health",
        ])
        .output()
        .expect("run recon");

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("valid JSON");

    let sections = json["sections"].as_array().expect("sections array");
    assert_eq!(sections.len(), 1, "only one section after filter");
    assert_eq!(sections[0]["id"], "health", "must be health section");
}

// ── AC09 — --source filter ────────────────────────────────────────────────

#[test]
fn test_source_filter() {
    let cfg = write_config(&minimal_config(concat!(
        "[[sources]]\nid = \"src1\"\nsection = \"health\"\ntype = \"shell\"\nargs = [\"echo\", \"a\"]\nformat = \"text\"\n",
        "[[sources]]\nid = \"src2\"\nsection = \"health\"\ntype = \"shell\"\nargs = [\"echo\", \"b\"]\nformat = \"text\"\n",
    )));
    let out = Command::new(recon_bin())
        .args([
            "run",
            "--config",
            cfg.path().to_str().unwrap(),
            "--source",
            "src1",
        ])
        .output()
        .expect("run recon");

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("valid JSON");

    let sections = json["sections"].as_array().expect("sections array");
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0]["sources"][0]["id"], "src1");
}

// ── AC11 — disabled sources excluded ─────────────────────────────────────

#[test]
fn test_disabled_source_excluded() {
    let cfg = write_config(&minimal_config(concat!(
        "[[sources]]\nid = \"enabled\"\nsection = \"health\"\ntype = \"shell\"\nargs = [\"echo\", \"yes\"]\nformat = \"text\"\nenabled = true\n",
        "[[sources]]\nid = \"disabled\"\nsection = \"health\"\ntype = \"shell\"\nargs = [\"echo\", \"no\"]\nformat = \"text\"\nenabled = false\n",
    )));
    let out = Command::new(recon_bin())
        .args(["run", "--config", cfg.path().to_str().unwrap()])
        .output()
        .expect("run recon");

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("valid JSON");

    assert_eq!(
        json["summary"]["sources_total"], 1,
        "sources_total must be 1 (disabled source excluded)"
    );

    // Verify "disabled" id is not present in any section
    let sections = json["sections"].as_array().expect("sections array");
    for sec in sections {
        for src in sec["sources"].as_array().expect("sources") {
            assert_ne!(src["id"], "disabled", "disabled source must not appear");
        }
    }
}

// ── AC12 — error source sets partial=true and has error object ────────────

#[test]
fn test_error_source_partial() {
    let cfg = write_config(&minimal_config(
        "[[sources]]\nid = \"fail\"\nsection = \"health\"\ntype = \"shell\"\nargs = [\"false\"]\nformat = \"text\"\n",
    ));
    let out = Command::new(recon_bin())
        .args(["run", "--config", cfg.path().to_str().unwrap()])
        .output()
        .expect("run recon");

    // exit code should be 1 (partial) or 3 (all failed)
    let code = out.status.code().unwrap_or(-1);
    assert!(code == 1 || code == 3, "failed source → exit 1 or 3, got {}", code);

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("valid JSON");

    assert_eq!(json["partial"], true, "partial must be true when source fails");

    let error_val = &json["sections"][0]["sources"][0]["error"];
    assert!(
        !error_val.is_null(),
        "error field must be present on failed source"
    );
}

// ── AC04 — recon check lists sources ─────────────────────────────────────

#[test]
fn test_check_lists_sources() {
    let cfg = write_config(&minimal_config(
        "[[sources]]\nid = \"good\"\nsection = \"health\"\ntype = \"shell\"\nargs = [\"echo\", \"ok\"]\nformat = \"text\"\n",
    ));
    let out = Command::new(recon_bin())
        .args(["check", "--config", cfg.path().to_str().unwrap()])
        .output()
        .expect("run recon check");

    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("good"),
        "check output must contain source id 'good', got:\n{}",
        stdout
    );
}

// ── AC05 — recon init --print outputs template ───────────────────────────

#[test]
fn test_init_print_template() {
    let out = Command::new(recon_bin())
        .args(["init", "--print"])
        .output()
        .expect("run recon init --print");

    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("schema_version"),
        "template must contain schema_version"
    );
    assert!(
        stdout.contains('#'),
        "template must contain comments"
    );
}
