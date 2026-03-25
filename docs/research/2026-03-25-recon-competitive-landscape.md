---
title: "recon Competitive Landscape & Design Inspiration"
date: 2026-03-25
run_id: 20260325T060618Z-95159
depth: medium
sources: 18+
verification_status: unverified
completion_status: complete
---

# recon — Competitive Landscape & Design Inspiration

## Executive Summary

Нет прямого конкурента, который бы делал именно то что планирует recon: **agent-first context aggregation CLI с pluggable shell sources для AI coding agents**. Но есть 6 категорий продуктов, из которых можно подсмотреть паттерны.

Ближайший конкурент — **CodeFire** (desktop app, MCP-based, 63 tools, persistent memory для Claude/Codex/Gemini). Но CodeFire — GUI desktop app, не CLI; монолит, не pluggable; ориентирован на memory, не на reconnaissance.

## 1. Terminal Dashboards (shell command aggregators)

### Sampler (14.5K stars, Go, archived 2023)
- **Что делает:** YAML-configured shell command execution + TUI visualization
- **Архитектура:** `sample:` field = shell command, `rate-ms:` = polling interval. 6 widget types (runchart, sparkline, barchart, gauge, textbox, asciibox). Triggers для алертов.
- **Что подсмотреть:**
  - YAML конфиг с `sample:` / `init:` / `transform:` pipeline на каждый source
  - `transform:` — постобработка output команды (jq-like)
  - Triggers: conditional actions на основе output (alert if value > threshold)
  - Interactive shell sessions (init → sample cycle для баз данных через SSH)
- **Чем recon отличается:** Sampler = TUI мониторинг, recon = one-shot context dump для агентов. Sampler визуализирует, recon структурирует JSON.
- **URL:** https://github.com/sqshq/sampler

### DevDash (1.6K stars, Go, archived 2023)
- **Что делает:** configurable terminal dashboard. Sources: GitHub, Google Analytics, Travis CI, SSH, local scripts.
- **Конфиг:** YAML/JSON/TOML, widget placement grid, per-widget colors и refresh.
- **Что подсмотреть:**
  - Multi-source конфиг с typed sources (github, ga, local script)
  - Widget placement system (row/col grid)
  - Built-in integrations + shell escape hatch
- **Чем recon отличается:** DevDash = TUI, recon = stdout pipe. DevDash рисует, recon агрегирует.
- **URL:** https://github.com/Phantas0s/devdash

### WTFUtil (16K stars, Go, active)
- **Что делает:** personal information dashboard in terminal. 30+ built-in modules.
- **Модули:** GitHub, Jira, Todoist, Google Calendar, OpsGenie, weather, stocks, etc.
- **Что подсмотреть:**
  - Module system с built-in + custom command modules
  - Каждый module = go interface, конфигурируется в YAML
  - Refresh intervals per module
  - Position grid system
- **Чем recon отличается:** WTFUtil = TUI dashboard, recon = CLI pipe. WTFUtil = Go modules, recon = shell commands.
- **URL:** https://github.com/wtfutil/wtf

**Pattern summary:** Все три мертвы/архивированы. Terminal dashboards — вымирающая категория. Но паттерн **"YAML/TOML config → shell command → periodic execution → structured output"** валиден и переживёт TUI обёртку.

---

## 2. AI Agent Memory / Context Persistence

### CodeFire (OSS, MIT, new)
- **Что делает:** Desktop companion для AI coding agents. Persistent memory, task tracking, semantic code search, browser automation. MCP server с 63 tools.
- **Поддержка:** Claude Code, Gemini CLI, Codex CLI, OpenCode.
- **Архитектура:** SQLite DB (shared schema), vector store для semantic search, MCP protocol. Swift (macOS native) + Electron (cross-platform).
- **Что подсмотреть:**
  - 63 MCP tools — какие именно context tools нужны агентам
  - Session history с cost analytics
  - Task tracking kanban как data source для agent context
  - Auto-project discovery
- **Ключевое отличие от recon:**
  - CodeFire = desktop GUI app (heavy, requires install)
  - recon = CLI pipe (lightweight, composable, zero GUI)
  - CodeFire = memory-first (remember decisions)
  - recon = reconnaissance-first (scan environment, report state)
  - CodeFire = MCP protocol (Claude Code specific integration)
  - recon = stdout JSON (any agent, any pipe)
- **URL:** https://github.com/websitebutlers/codefire-app

### ContextStream ($20/mo, proprietary)
- **Что делает:** Persistent memory layer for Cursor, Claude Code, VS Code. Remembers decisions, preferences, project context across sessions.
- **Подход:** MCP-based, cloud-synced. Decision tracking, timeline view, lesson capture, team sharing.
- **Что подсмотреть:**
  - "Decision tracking with full rationale" — полезная category для recon sources
  - "Timeline view of project evolution" — recon --diff mode
  - "Natural language memory commands" — SKILL.md pattern
- **Чем recon отличается:** ContextStream = SaaS memory layer, recon = local CLI aggregator. ContextStream синхронизирует память, recon собирает разведку.

### MCP Backpack (OSS)
- **Что делает:** MCP server для persistent portable memory. Two-layer storage: DiskCache (SQLite, local) + backpack.json (git-committed, portable).
- **Что подсмотреть:**
  - "Pack on machine A, git push, git pull on machine B, unpack" — portable context
  - Two-layer: hot (local SQLite) + cold (git-tracked JSON)
  - Per-project memory scope
- **URL:** https://medium.com/codex/introducing-mcp-backpack-persistent-portable-memory-for-ai-coding-agents-87eea16eaa54

### OpenMemory / Mem0 (OSS)
- **Что делает:** Persistent MCP memory layer для coding agents. Knowledge graph + vector search.
- **Что подсмотреть:**
  - Structured memory with graph relationships
  - Autonomous consolidation (merge related memories)

### Engram (OSS, Go, already in use)
- **Что делает:** Persistent memory for AI coding agents. SQLite+FTS5, MCP server, HTTP API, CLI, TUI.
- **URL:** https://github.com/Gentleman-Programming/engram
- **Примечание:** Уже используется в vicc stack. recon может читать Engram как source.

**Pattern summary:** Memory tools = MCP-based, per-session persistence. recon occupies a different niche: **pre-session reconnaissance** (scan ALL sources, not just memory). Memory = remember past. recon = see present.

---

## 3. Standup/Daily Report Automation

### standup-helper (0 stars, Python)
- **Что делает:** Aggregates git commits, Jira tickets, daily work notes, timewarrior → sends to Vertex AI → generates formatted standup summary.
- **Архитектура:** modular (clients/ for APIs, services/ for aggregation). Config: secrets.env + config.ini.
- **Что подсмотреть:**
  - Multi-source aggregation → AI synthesis pipeline (exactly recon → LLM flow)
  - Sources: git log, Jira API, file notes, timewarrior — same idea as recon sources
  - Structured intermediate format before AI formatting
- **Чем recon отличается:** standup-helper = Python, hardcoded sources, human output. recon = Rust, pluggable sources, agent output.

### Steady (SaaS)
- **Что делает:** Connects GitHub + Jira → auto-pulls commits, PRs, ticket progress into standup updates. Distributes via Slack/Teams/email.
- **Что подсмотреть:** Auto-aggregation pattern: pull recent activity from git + tracker → summarize.

### Standuply (SaaS)
- **Что делает:** Async standups via Slack/Teams, connected to Jira + GitHub for auto-reports.
- **Что подсмотреть:** Time-windowed activity aggregation (last 24h).

**Pattern summary:** Standup tools aggregate git + tracker + notes → summary. recon does the same but: wider sources, agent-first output, CLI-native, pluggable.

---

## 4. Context Engineering / Agent Orchestration

### Oxicrab (OSS, Rust)
- **Что делает:** Multi-platform AI assistant framework. Key feature: **context providers** — shell commands that inject live context into every LLM turn.
- **Архитектура:** Rust crate workspace. Layered TOML config. 29 built-in tools. SQLite memory.
- **Context providers:**
  - Shell commands execute before each LLM turn
  - Cached with configurable TTL
  - Dependency-checked (validates command exists)
  - Silently skip on failure
  - Inject git status, env info, external data into system prompt
- **Что подсмотреть (VERY RELEVANT):**
  - **Context provider model** — exactly recon's core idea but embedded in an assistant framework
  - **TTL caching** — don't re-run expensive commands every time
  - **Dependency checking** — validate `gh`, `tw`, etc. exist before running
  - **Graceful skip** — failed sources don't crash the pipeline
  - **Three-layer routing** — deterministic → constrained LLM → full LLM fallback
- **Чем recon отличается:** Oxicrab = embedded context in an assistant framework. recon = standalone CLI that ANY agent can call. Oxicrab runs context providers per-turn, recon runs them on-demand.
- **URL:** https://oxicrab.dev/

### AGENTS.md (spec/format)
- **Что делает:** Markdown format for briefing AI coding agents. Repository-level context file loaded at task start.
- **Что подсмотреть:**
  - Standard format for agent context = recon's output format inspiration
  - Static (file) vs dynamic (recon runs commands to collect live data)
- **URL:** https://agents.md/

### Anthropic Context Engineering Guide
- **Что делает:** Official guide on building effective context for AI agents.
- **Key insight:** "What configuration of context is most likely to generate our model's desired behavior?" — this IS recon's mission.
- **URL:** https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents

**Pattern summary:** Context engineering = the academic/industry frame for what recon does. Oxicrab's context providers are the closest architectural match.

---

## 5. Parallel Command Runners

### rust-parallel (Rust, tokio)
- **Что делает:** Fast CLI to run commands in parallel. Similar to GNU parallel.
- **Что подсмотреть:**
  - tokio-based parallel command execution
  - Output aggregation from parallel processes
  - Timeout handling per command
- **URL:** https://github.com/aaronriekenberg/rust-parallel

### config-rs (Rust)
- **Что делает:** Layered configuration system for Rust apps. TOML/YAML/JSON/ENV.
- **Что подсмотреть:**
  - Layered config: defaults → file → env → CLI args
  - Strong typing with serde
- **URL:** https://github.com/rust-cli/config-rs

---

## 6. AI Daily Planning

### TaskFlow AI, DeyWeaver, Plan-Agent
- **Что делают:** AI-powered daily task planners. Input tasks → AI breaks down → generates schedule.
- **Что подсмотреть:**
  - `recon plan` future command could follow this pattern
  - Task decomposition → time estimation → schedule generation
  - Integration with real task sources (not just manual input)

---

## Competitive Matrix

| Tool | Type | Config | Sources | Output | Agent-first | Pluggable | Status |
|------|------|--------|---------|--------|-------------|-----------|--------|
| **recon** | CLI | TOML | shell + file | JSON/md | **YES** | **YES** | planned |
| Sampler | TUI | YAML | shell | visual | no | yes (shell) | archived |
| DevDash | TUI | YAML | shell + APIs | visual | no | partial | archived |
| WTFUtil | TUI | YAML | 30+ modules | visual | no | modules | active |
| CodeFire | Desktop | built-in | MCP | MCP | yes | no | active |
| ContextStream | SaaS | built-in | MCP | MCP | yes | no | active |
| Oxicrab | Framework | TOML | shell (ctx) | LLM prompt | embedded | yes | active |
| standup-helper | Script | INI | git+jira | text | no | no | inactive |

## Key Insights

### 1. Пустая ниша
Нет CLI tool который бы: (a) собирал context из pluggable sources, (b) выдавал structured JSON, (c) был agent-agnostic. CodeFire ближе всего, но это desktop app с 63 MCP tools — overkill для "дай мне контекст утром".

### 2. Terminal dashboards мертвы, pattern жив
Sampler (14.5K), DevDash (1.6K), WTFUtil (16K) показали что people want configurable shell-command dashboards. Все три TUI → все три устарели. recon берёт core pattern (TOML config → shell commands → aggregate) без мёртвого TUI.

### 3. Oxicrab — closest architectural match
Context providers (shell commands → cached → injected per-turn) это exactly recon's model. Разница: Oxicrab embeds this in a framework, recon extracts it as standalone CLI. **Подсмотреть: TTL caching, dependency checking, graceful skip.**

### 4. Sampler's transform pipeline — steal this
`init:` → `sample:` → `transform:` три-шаговый pipeline на каждый source. recon может добавить `transform:` field для jq-style постобработки.

### 5. standup-helper validates the aggregation→AI flow
Git + Jira + notes → AI summary = exactly `recon run | llm "plan my day"`. Но standup-helper hardcoded. recon = pluggable version.

### 6. Agent dispatch = next frontier
Plan-Agent, DeyWeaver показывают что "AI plans your day from tasks" is a thing. `recon plan` → `recon dispatch` = natural evolution, но post-v1.0.

## Design Recommendations for recon

1. **Steal Sampler's config model:** `sample:` (command) + `transform:` (post-process) + `rate:` (freshness)
2. **Steal Oxicrab's provider model:** TTL caching, dependency check, graceful skip
3. **Steal standup-helper's flow:** aggregate → structured intermediate → AI synthesis
4. **Don't build TUI** — terminal dashboards are dead; JSON stdout is the new dashboard
5. **Don't build MCP server** (v1) — MCP is platform-specific; stdout JSON is universal
6. **Name the output format** — make "recon report" a defined schema that agents can learn once
7. **Add `transform` field** — `transform = "jq '.[] | {title, url}'"` for per-source output filtering
8. **Add `freshness_check` command** — like Oxicrab's dependency check, but for data age

## Sources

- [CodeFire](https://codefire.app/) — Desktop companion for AI coding agents
- [Sampler](https://github.com/sqshq/sampler) — 14.5K stars, YAML shell command dashboard
- [DevDash](https://github.com/Phantas0s/devdash) — 1.6K stars, configurable terminal dashboard
- [WTFUtil](https://wtfutil.com/) — 16K stars, personal information dashboard
- [Oxicrab](https://oxicrab.dev/) — Rust AI assistant with context providers
- [ContextStream](https://contextstream.io/) — $20/mo persistent AI memory
- [MCP Backpack](https://medium.com/codex/introducing-mcp-backpack-persistent-portable-memory-for-ai-coding-agents-87eea16eaa54) — Portable memory via git
- [standup-helper](https://github.com/daniel-mcdonough/standup-helper) — Git+Jira→AI standup summary
- [AGENTS.md](https://agents.md/) — Standard format for agent context
- [Anthropic Context Engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
- [rust-parallel](https://github.com/aaronriekenberg/rust-parallel) — Parallel command execution in Rust
- [config-rs](https://github.com/rust-cli/config-rs) — Layered Rust configuration
- [OpenMemory/Mem0](https://mem0.ai/openmemory) — MCP memory layer
- [Engram](https://github.com/Gentleman-Programming/engram) — Go persistent memory, already in stack
