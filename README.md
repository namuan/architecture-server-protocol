# Architecture Server Protocol (ASP)

A code architecture analysis and enforcement tool for multi-language projects. ASP lets you define logical components, ownership, and constraints in an `architecture.toml` file, then validates that your codebase adheres to those constraints — with continuous file watching and optional AI-assisted explanations.

## Features

- **Architectural indexing** — Parses Python, JavaScript, and TypeScript files into a local SQLite database with imports, symbols, and component ownership
- **Constraint enforcement** — Detects circular dependencies (via Tarjan's SCC algorithm) and unowned files
- **Dependency graph visualization** — Export component graphs as text, DOT, or JSON
- **File watching** — Incrementally re-indexes changed files with 300ms debouncing
- **AI explanations** — Streams LLM-generated rationale for files and components via any OpenAI-compatible endpoint (Ollama, Claude API, etc.)

## Installation

Requires Rust and `cargo` (install via [rustup.rs](https://rustup.rs)). Clone the repo, then:

```bash
cargo install --path crates/asp-cli
```

## Quick Start

```bash
# 1. Initialize ASP in your project (builds the index)
cd my-project
asp init

# 2. Edit the generated architecture.toml to define components and rules

# 3. Check for violations
asp check
```

## Commands

| Command | Description |
|---------|-------------|
| `asp init` | Index the project and write a skeleton `architecture.toml` |
| `asp check` | Run the rule engine and report violations |
| `asp graph` | Visualize component dependency graph |
| `asp explain <path>` | Stream an LLM explanation of a file or component |
| `asp watch` | Watch for file changes and re-index incrementally |
| `asp refresh` | Re-run LLM inference on stale components *(stub — not yet implemented)* |
| `asp status` | Show index statistics (files, imports, symbols, DB path) |

**Note:** `asp check`, `asp graph`, `asp explain`, and `asp status` require `asp init` to have been run first.

### Options

```
asp init    [--project-root <dir>] [--yes]
asp check   [--rule <name>] [--format text|json]
asp graph   [--format text|dot|json]
asp explain <path>
asp watch   [--format text|json]
asp refresh [--force]
asp status
```

## Configuration

### architecture.toml

Defines logical components, path ownership, and architectural rules. `asp init` writes a skeleton — edit it to match your project:

```toml
[project]
name = "my-project"
languages = ["python"]
version = "1"

[[component]]
name = "api"
description = "HTTP handlers and routing"
paths = ["src/api/**"]
owner = "team-backend"

[[component]]
name = "db"
description = "Database models and queries"
paths = ["src/db/**"]

[[rule]]
name = "no-cycles"
type = "no_cycles"

[[rule]]
name = "no-unowned"
type = "no_unowned"
```

**Component paths** use glob patterns. Each file is matched to at most one component.

**Built-in rule types:**

| Rule | Description |
|------|-------------|
| `no_cycles` | Detects circular dependencies between components |
| `no_unowned` | Warns about files not assigned to any component |

### Global config (`~/.asp/config.toml`)

```toml
[llm]
base_url = "http://localhost:11434/v1"  # default: Ollama
model = "llama3.1:8b"
api_key = ""

[staleness]
threshold = 0.15  # fraction of changed files before refresh is suggested
```

## How It Works

ASP parses source files with tree-sitter, storing imports and symbols in a per-project SQLite database under `~/.asp/`. Files are hashed (SHA256) so only changed files are re-parsed on subsequent runs. The resolver matches files to components via glob patterns and builds a dependency graph; the rule engine then checks that graph for violations.

Each project gets a unique database filename derived from the project path, so multiple projects can be indexed simultaneously.

## Workspace Structure

```
crates/
  asp-cli      — CLI entry point and command definitions
  asp-core     — Indexer, resolver, and manifest loading
  asp-db       — SQLite schema and migrations
  asp-parser   — Tree-sitter parsers for Python, JS, and TS
  asp-rules    — Rule engine and cycle detection
  asp-watcher  — Filesystem watcher with debouncing
  asp-llm      — OpenAI-compatible async streaming LLM client
  asp-daemon   — JSON-RPC 2.0 Unix socket daemon (experimental)
```

## License

MIT — see [LICENSE](LICENSE).
