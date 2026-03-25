# recon — Project Intent

## What is recon?

A Rust CLI that aggregates context from multiple configurable sources into a unified briefing for AI coding agents. The primary consumer is an AI agent (Claude Code, Codex, Gemini CLI), not a human.

## Problem

AI coding agents start each session blind — they don't know what happened yesterday, what tasks are pending, what's blocked, what news is relevant to current work. Context is scattered across dozens of tools (GitHub, email, task managers, memory systems, metrics, news feeds). Manually assembling this context wastes the first 5-10 minutes of every agent session.

## Solution

`recon run` executes configured shell commands in parallel, parses their output, and emits a structured JSON briefing. A thin SKILL.md tells any AI agent how to call recon and interpret the output.

## Non-Goals

- NOT a human dashboard or TUI
- NOT a cron daemon or background service
- NOT a replacement for individual tools (GitHub CLI, taskwarrior, etc.)
- NOT an AI agent itself — it collects context, agents consume it
- NOT tightly coupled to any specific AI agent runtime

## Core Principles

1. **Shell commands as universal connectors** — any CLI tool becomes a source via TOML config
2. **Machine-readable first** — JSON output primary, human-readable secondary
3. **Zero mandatory dependencies** — `cargo install recon` works with no API keys or tokens
4. **Parallel by default** — sources execute concurrently with per-source timeouts
5. **Fail gracefully** — unavailable sources degrade to warnings, not errors

## Target Users

1. Developers using AI coding agents who want persistent context across sessions
2. Teams running multiple AI agents across multiple projects
3. (Future) Automated agent dispatch systems that need daily work plans
