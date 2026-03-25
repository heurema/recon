# recon Roadmap

**Created:** 2026-03-25 | **Last updated:** 2026-03-25
**Goal:** Working agent context pipe, published to crates.io
**Research:** docs/research/2026-03-25-recon-cli-brainstorm.md
**Competitive landscape:** docs/research/2026-03-25-recon-competitive-landscape.md
**Panel review 1:** Codex (gpt-5.4) + Gemini — 12 gaps found, all addressed
**Panel review 2:** Codex + Gemini — readiness check. 7 blocking questions resolved

---

## v0.1.0 — MVP (target: 1-2 weeks)

**Subcommands:** `recon check` + `recon run` + `recon init --print`

### Security: Trust Model
- [ ] Config loading: **global only by default** (`~/.config/recon/briefing.toml`)
- [ ] Local config only via explicit `--config ./briefing.toml` flag
- [ ] `RECON_CONFIG` env var override
- [ ] Document: **untrusted config = untrusted code execution**
- [ ] No `shell = true` in v0.1 — args array only
- [ ] No stdin inheritance to child processes
- [ ] Kill **process group** on timeout (`setsid` + `killpg`), not just child pid

### Config Schema
- [ ] `schema_version = 1` in TOML config (top-level, required)
- [ ] Source types: `shell` (default) + `file`
- [ ] `~` expansion in `path` fields only (NOT in `args` — documented)
- [ ] Per-source `on_error`: `"warn"` (default) | `"fail"` | `"omit"`
- [ ] Per-source `timeout_sec` override (default from `[defaults]`; ignored for `file` type)
- [ ] `max_output_bytes` per source (default from `[defaults]`, fallback 1MB)
- [ ] Duplicate source ID detection → config error

TOML validation matrix:
- `type = "shell"`: `args` required (non-empty), `path` forbidden. `args[0]` checked in PATH at runtime.
- `type = "file"`: `path` required, `args` forbidden. `~` expanded. File existence checked at runtime.
- Both: `id` required (unique), `section` required (one of: health|actions|code|comms|context|ideas), `format` required (one of: json|jsonl|text|markdown)
- `args = []` → config error. `timeout_sec = 0` → config error. `timeout_sec` on `file` type → ignored (no warning).
- Invalid UTF-8 in file/stdout → `parse_error`

### Core Execution
- [ ] Parallel execution: tokio spawn per source, per-source timeout
- [ ] Dependency check at runtime: if `args[0]` not found → structured error, not crash
- [ ] Distinguish error types: `command_not_found` | `command_failed` | `timed_out` | `parse_error` | `file_not_found` | `output_too_large`
- [ ] `on_error = "omit"`: source completely absent from output, not counted in summary
- [ ] `on_error = "fail"`: finish in-flight sources, emit partial JSON, exit code 3
- [ ] `on_error = "warn"` (default): source in output with error details, exit code 1
- [ ] Always parallel execution (no config toggle, always tokio concurrent)
- [ ] stdout = JSON only, all diagnostics → stderr
- [ ] `--verbose` writes to stderr, never breaks JSON output
- [ ] Deterministic ordering: sections in fixed order, sources in config order

### Output Schema (JSON)
```json
{
  "schema_version": "0.1",
  "generated_at": "2026-03-25T07:00:00Z",
  "duration_ms": 842,
  "partial": true,
  "config": {
    "path": "/Users/vi/.config/recon/briefing.toml",
    "scope": "global"
  },
  "summary": {
    "sources_total": 9,
    "sources_ok": 7,
    "sources_failed": 1,
    "sources_timed_out": 1
  },
  "sections": [
    {
      "id": "actions",
      "title": "Pending Actions",
      "sources": [
        {
          "id": "taskwarrior",
          "type": "shell",
          "content_type": "json",
          "status": "ok",
          "duration_ms": 150,
          "data": [{"id": 1, "description": "..."}],
          "error": null
        },
        {
          "id": "beads-issues",
          "type": "shell",
          "content_type": "json",
          "status": "error",
          "duration_ms": 50,
          "data": null,
          "error": {
            "type": "command_failed",
            "message": "Exit code 1",
            "exit_code": 1,
            "stderr": ".beads directory not found"
          }
        }
      ]
    }
  ]
}
```

Key contracts:
- No `confidence` field — use raw `summary` counts + `partial` flag instead
- Per-source provenance preserved (not flattened into section items)
- `data`: any valid JSON value for json (object, array, etc), `Vec<Value>` for jsonl, `String` for text/markdown, `null` on error
- `content_type`: `"json"` | `"jsonl"` | `"text"` | `"markdown"`
- `trust`: always `"untrusted"` — all external data is untrusted by definition
- `status`: `"ok"` | `"error"` | `"timed_out"`
- Sources with `on_error = "omit"` are completely absent from output
- Sources with `enabled = false` are excluded; not counted in `summary.sources_total`
- `--section`/`--source` filters post-collection; summary reflects filtered view
- `config.scope`: `"global"` | `"explicit"` | `"env"`
- `error.stderr`: truncated to 1KB, no secrets redaction in v0.1
- `--format text`: wrap each source data in `<external_data source="id" trust="untrusted">` tags

### Subcommands
- [ ] `recon run` — collect all sources, render JSON to stdout
- [ ] `recon run --format text` — human-readable markdown to stdout
- [ ] `recon run --section <id>` — filter to specific section
- [ ] `recon run --source <id>` — run single source only
- [ ] `recon check` — validate config + verify binaries + optional exec with preview (text output only in v0.1)
- [ ] `recon init --print` — print well-commented example config to stdout

### CLI Flags (global)
- [ ] `--config <path>` — explicit config file
- [ ] `--verbose` — debug output to stderr
- [ ] `--format json|text` — output format (default: json)

### Exit Codes
- `0` — success (all sources ok)
- `1` — partial success (some sources failed, `partial: true`)
- `2` — config error (invalid TOML, missing file, etc.)
- `3` — fatal error (no sources could run)

### Examples
- [ ] Example: GitHub PRs (`gh pr list --json title,url,author,updatedAt`)
- [ ] Example: Taskwarrior (`task export +PENDING +READY`)
- [ ] Example: file reader (Gmail summary JSON)
- [ ] Example: markdown file (agent-brief.md, directive.md)
- [ ] Fix: no `~` in args (use absolute paths or `$HOME` via env)
- [ ] Fix: `gh api --jq` returns stream → use `format = "jsonl"` or wrap in array

### Quality
- [ ] Integration tests: mock sources with shell echo commands
- [ ] Config validation with clear error messages
- [ ] Platform: Unix-only for v0.1 (macOS + Linux), document explicitly

---

## v0.2.0 — Freshness & Extensions

- [ ] `freshness_sec` + `freshness_field` per source → `"fresh"` | `"stale"` | `"unknown"` markers
- [ ] `freshness_field` works for both `file` and `shell` sources (if stdout is JSON with timestamp)
- [ ] `glob` source type: pick most recently modified file matching pattern
- [ ] `glob_mode = "latest"` (default) | `"all"` | `"concat"`
- [ ] `cwd` per source — working directory for shell commands
- [ ] `env` per source — extra env vars for child process (`env = { GH_TOKEN = "$GH_TOKEN" }`)
- [ ] `transform` field — jq expression for post-processing JSON output
- [ ] `display_if_empty = false` — omit source from output if data is empty array/null
- [ ] `recon run --diff` — content hash per source, show only changes since last run
- [ ] Cache: `~/.cache/recon/last.json` for diff baseline

---

## v0.3.0 — Init & Shell

- [ ] `recon init` — full auto-detection:
  - Repo-level: `.git/config` (→ gh), `.beads/` (→ beads), known state file patterns
  - User-level: `gh`, `task`, `beads`, `engram` in PATH + auth checks (`gh auth status`)
  - File-level: JSON/JSONL/md in known dirs, freshness by mtime
  - Unconfirmed sources → `enabled = false` with comment
- [ ] `recon init --dir <path>` — scan specific directory
- [ ] `recon init --interactive` — prompt user for each detected source
- [ ] `shell = true` opt-in for pipe/redirect sources
  - One-time non-suppressible stderr warning on first `shell = true` execution
  - `command = "..."` syntax only when `shell = true`
- [ ] Config merging: global + local (when `--allow-local-config` flag set)
- [ ] `sensitive = true` per source — redact data preview in `check` output

---

## v0.4.0 — Agent Integration

- [ ] `recon plan` — synthesize daily work plan from collected data (requires LLM, thin wrapper)
- [ ] `recon generate-skill` — output agent-specific SKILL.md
- [ ] Per-project profiles: `recon run --profile work` with named config sets

---

## v1.0.0 — Public Release

- [ ] Publish to crates.io
- [ ] README with examples for 5+ popular tools (gh, task, beads, kubectl, curl)
- [ ] Blog post on ctxt.dev
- [ ] Validate with 1+ external user
- [ ] JSON schema file for output contract (published alongside crate)

---

## Future (post-1.0, ideas)

- [ ] `recon dispatch` — generate agent work plan, launch agents to execute
- [ ] `recon watch` — long-running JSONL stream mode
- [ ] `recon doctor` — comprehensive environment audit (PATH, auth, permissions, cache)
- [ ] Typed connectors (trait-based) for sources that benefit from structured parsing
- [ ] Team mode: shared config + aggregated team briefing

---

## Architecture Decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Connector model | Shell commands in TOML | Universal, zero-dep, any CLI tool works |
| Command format | `args = [...]` array | No shell injection (vs `command = "string"`) |
| Output primary | JSON to stdout | Agent-first, pipe-friendly |
| Diagnostics | stderr only | `--verbose` must not break JSON output |
| Trust model | Global config default, local via `--config` | Prevent arbitrary code exec from untrusted repos |
| Storage | Stateless (stdout), optional cache for diff | Simplicity; no daemon, no DB |
| Distribution | Single binary, crates.io | Minimal install friction |
| Config format | TOML | Rust ecosystem standard, human-editable |
| Async runtime | tokio | Parallel source execution with timeouts |
| Process cleanup | Kill process group (setsid+killpg) | Prevent orphan child processes on timeout |
| `~` expansion | In path/glob fields only, NOT in args | Explicit, predictable, no hidden magic |
| Confidence score | Deferred to v0.2+ | Raw counts + partial flag more honest for v0.1 |

---

## Changelog

- 2026-03-25 (v3): Panel review 2 (readiness check). 7 blocking questions resolved:
  data=any JSON value, on_error semantics frozen, trust field added, check --format json
  deferred, parallel field removed, disabled/filtered summary rules defined. SKILL.md synced.
- 2026-03-25 (v2): Panel review 1 (Codex gpt-5.4 + Gemini): 12 gaps found and addressed.
  Key changes: trust model added, init --print pulled to v0.1, per-source errors in JSON,
  schema_version, confidence removed, process group kill, ~ expansion rules, exit codes.
- 2026-03-25 (v1): Created from brainstorm + competitive landscape research.
