# mcp-scanner

Repository Scanner MCP server for the Doki Stack platform. Scans git repositories to contextualize the AI — clones repos, analyzes code, generates summaries via LLM, and stores structured context.

## Purpose

The Scanner MCP contextualizes the AI agents by scanning git repositories. When a repository is pushed or manually triggered, this service clones the repo, analyzes its structure (technologies, naming patterns, IaC resources), calls the LLM to generate human-readable summaries, and stores the results for agents to consume.

## Technology Stack

| Component | Technology |
|-----------|-----------|
| Language | Rust (2021 edition) |
| Web Framework | axum 0.7 |
| Async Runtime | tokio |
| Database | sqlx (PostgreSQL) |
| Git Operations | git2 (libgit2 bindings) |
| Object Storage | aws-sdk-s3 (MinIO compatible) |
| Cache | redis-rs (Dragonfly compatible) |
| Message Queue | lapin (RabbitMQ AMQP) |
| HTTP Client | reqwest (LLM API calls) |
| Shared Crate | doki-shared (shared-rust) |

## MCP Tools

| Tool | Description |
|------|-------------|
| `scan` | Trigger a full repository scan (clone → analyze → summarize → store) |
| `get-context` | Retrieve the latest scan context for a repository |

## Key Behaviors

- Consumes GitHub/GitLab webhooks via RabbitMQ
- Git clone using git2 with sparse checkout and shallow clone for performance
- File tree walking with tokio::fs for async I/O
- LLM summarization via Ollama/vLLM to generate skill.md and instructions.md
- Rate limiting: 1 scan per repo per 5 minutes, 10 concurrent scans per org

## Implementation Phase

**Phase 1** (Weeks 9-12) — Built after shared-rust and db-schemas are stable.

## License

Apache License 2.0 — see [LICENSE](LICENSE)
