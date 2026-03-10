# mcp-scanner — High-Level Design

## Overview

The Scanner MCP is a Rust service that contextualizes AI agents by scanning git repositories. It produces structured summaries (skill.md, instructions.md) that agents use to understand what a repository contains and how it should be managed.

## Architecture

```
mcp-scanner/
├── src/
│   ├── main.rs                # Entry point, axum router, graceful shutdown
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── scan.rs            # POST /mcp/v1/tools/scan
│   │   └── context.rs         # POST /mcp/v1/tools/get-context
│   ├── services/
│   │   ├── mod.rs
│   │   ├── cloner.rs          # Git clone via git2
│   │   ├── analyzer.rs        # File tree analysis, tech detection
│   │   ├── summarizer.rs      # LLM call to generate summaries
│   │   └── storage.rs         # MinIO upload, PG index, Dragonfly cache
│   ├── consumers/
│   │   └── webhook.rs         # RabbitMQ webhook consumer
│   ├── models/
│   │   ├── mod.rs
│   │   ├── scan.rs
│   │   └── context.rs
│   └── config.rs
├── Cargo.toml
├── Cargo.lock
├── Dockerfile
└── openapi.yaml
```

## Data Flow

```
GitHub/GitLab webhook → RabbitMQ → webhook consumer
  → git2 clone (sparse, shallow)
  → walkdir + tokio::fs analysis
  → reqwest call to Ollama/vLLM (generate skill.md, instructions.md)
  → Validate against context_schema.json
  → Upload to MinIO (org_id={org_id}/{repo}/{commit_sha}/)
  → Upsert scanner_context in PostgreSQL
  → Cache in Dragonfly (24h TTL)
```

## Rate Limiting

| Limit | Value |
|-------|-------|
| Per-repo scan frequency | 1 scan per 5 minutes |
| Concurrent scans per org | 10 maximum |
| Clone timeout | 60 seconds |
| LLM summarization timeout | 120 seconds |

## Dependencies

| Dependency | Type |
|-----------|------|
| `shared-rust` | Rust crate |
| `db-schemas` | SQL migrations |
| MinIO | Object storage (runtime) |
| PostgreSQL | Scan index (runtime) |
| Dragonfly | Cache (runtime) |
| RabbitMQ | Webhook events (runtime) |
| Ollama/vLLM | LLM for summarization (runtime) |
