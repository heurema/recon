/// Returns a well-commented TOML config template.
pub fn template() -> &'static str {
    r#"# recon briefing config — schema_version = 1
# Place this file at ~/.config/recon/briefing.toml or pass via --config
schema_version = 1

[defaults]
# Default execution timeout per source (seconds)
timeout_sec = 10
# Maximum output size per source (bytes)
max_output_bytes = 102400

# ── Sources ─────────────────────────────────────────────────────────────────
# Each [[sources]] entry defines one data source.
# Required fields: id, section, type, format
# Optional fields: args, path, timeout_sec, on_error, enabled
#
# Sections: health | actions | code | comms | context | ideas
# Types:    shell  | file
# Formats:  text   | markdown | json | jsonl
# on_error: warn (default) | fail | omit

# ── Health ───────────────────────────────────────────────────────────────────
# [[sources]]
# id      = "gh-status"
# section = "health"
# type    = "shell"
# args    = ["gh", "api", "rate_limit", "--jq", ".rate"]
# format  = "json"

# ── Actions ──────────────────────────────────────────────────────────────────
# [[sources]]
# id      = "gh-prs"
# section = "actions"
# type    = "shell"
# args    = ["gh", "pr", "list", "--json", "number,title,state"]
# format  = "json"

# [[sources]]
# id      = "gh-issues"
# section = "actions"
# type    = "shell"
# args    = ["gh", "issue", "list", "--limit", "20"]
# format  = "text"

# ── Code ─────────────────────────────────────────────────────────────────────
# [[sources]]
# id      = "git-status"
# section = "code"
# type    = "shell"
# args    = ["git", "status", "--short"]
# format  = "text"

# [[sources]]
# id      = "git-log"
# section = "code"
# type    = "shell"
# args    = ["git", "log", "--oneline", "-10"]
# format  = "text"

# ── Comms ────────────────────────────────────────────────────────────────────
# [[sources]]
# id      = "unread-mail"
# section = "comms"
# type    = "shell"
# args    = ["gws", "mail", "list", "--unread", "--limit", "10"]
# format  = "json"

# ── Context ──────────────────────────────────────────────────────────────────
# [[sources]]
# id      = "agent-brief"
# section = "context"
# type    = "file"
# path    = "~/vicc/state/agent-brief.md"
# format  = "markdown"

# [[sources]]
# id      = "project-notes"
# section = "context"
# type    = "file"
# path    = "~/notes/context.md"
# format  = "markdown"

# ── Ideas ────────────────────────────────────────────────────────────────────
# [[sources]]
# id      = "ideas-backlog"
# section = "ideas"
# type    = "shell"
# args    = ["task", "export", "project:ideas"]
# format  = "json"
# on_error = "omit"
"#
}
