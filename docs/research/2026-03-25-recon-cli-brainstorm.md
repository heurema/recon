# recon — Agent-First Context Pipe CLI

**Date:** 2026-03-25
**Status:** brainstorm complete, pre-implementation
**Source:** 3-persona brainstorm (Explorer/Operator/Contrarian), 18 idea cards, 3 branches interrogated

## Vision (refined post-brainstorm)

recon is an **agent-first** CLI tool. Primary consumer — AI agents (Claude Code, Codex, Gemini CLI).
NOT a human morning dashboard. The purpose:

1. What did agents not finish yesterday? Where did sessions stop?
2. What tasks exist (tw, beads, GitHub issues/PRs)?
3. What news/ideas are relevant to current projects?
4. Propose a daily work plan for AI agents
5. (Future) Dispatch agents to execute the plan

Output: structured JSON (primary) + human-readable markdown (secondary, via `--format text`).
Thin SKILL.md wrapper makes it universal for any AI agent.

## Architecture Decision: Shell Runner

**Core = TOML config + parallel shell command execution + structured output.**

Each source is a shell command defined in `briefing.toml`:
```toml
[[source]]
id = "github-prs"
section = "code"
args = ["gh", "pr", "list", "--json", "title,url,author,updatedAt"]
format = "json"
timeout_sec = 10
freshness_sec = 3600
```

Key decisions:
- `args = [...]` array, NOT `command = "string"` (no shell injection)
- `shell = true` explicit opt-in for pipes/redirects
- Parallel execution with per-source timeouts from MVP
- Built-in types: `file` (read JSON/JSONL/md), `glob` (aggregate files), `shell` (default)
- Typed intermediate struct for parsed output, not raw strings

## Subcommands (build order)

1. `recon check` — validate config, test each source, show timing
2. `recon run` — collect all sources, render output
3. `recon init` — auto-detect files, generate starter TOML
4. `recon plan` — (future) synthesize daily work plan from collected data
5. `recon dispatch` — (future) launch agents to execute plan

## Output Contract

```json
{
  "generated_at": "2026-03-25T07:00:00Z",
  "confidence": 0.85,
  "sections": [
    {
      "id": "code",
      "title": "Code",
      "freshness": "fresh",
      "items": [...]
    }
  ]
}
```

## Open Questions

1. Per-repo `.recon.toml` vs global `~/.config/recon/`? (start global)
2. Who is user #2? Validate before v0.1.0
3. Session history source — engram MCP? git log? agent session files?
4. Diff mode (--diff) — show only changes since last run

## Key Risks

- Shell injection if `shell = true` used carelessly — document prominently
- Latency compounding — parallel exec + timeouts mandatory
- Positioning: "agent context pipe" is niche — need clear README lede
- Duplicating materialize.py without clear win — must articulate the delta

## Prior Art & Differentiation

- wtfutil: human dashboard, no agent output, no shell connectors
- morning.sh: shell scripts, no structured output, no config
- materialize.py (own): Python, hardcoded sources, no config, not publishable
- recon: configurable sources, structured JSON output, agent-first, publishable

## Future: Agent Dispatch (idea, not MVP)

recon collects context → synthesizes daily plan → user approves → recon dispatches
agents (Claude Code teams, Codex, Gemini) to execute plan items in parallel.
This is the endgame but requires: working recon MVP + agent team APIs + approval UX.
Save as I-type idea, revisit after recon v0.2.0.
