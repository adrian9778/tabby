# 第十一章：贡献指南

## 11.1 Development Setup

### Required Toolchain

| 工具 | 版本要求 | 用途 |
|------|---------|------|
| **Rust** | 1.70+ (stable) | Server backend compilation |
| **cargo** | 1.70+ | Rust package manager & build tool |
| **Node.js** | 20+ | EE frontend (tabby-ui) build |
| **pnpm** | 8+ | TypeScript monorepo package manager |
| **protoc** | libprotoc 4.x+ | protobuf compilation (for sqlx-migrate) |
| **SQLite3** | 3.30+ | Database CLI tool |

### One-Line Setup (Ubuntu/Debian)

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Dependencies
sudo apt update
sudo apt install -y protobuf-compiler libopenblas-dev graphviz sqlite3

# Node.js 20
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo bash -
sudo apt install -y nodejs

# pnpm
npm install -g pnpm
```

### One-Line Setup (macOS)

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Dependencies
brew install protobuf openblas graphviz sqlite3
# Node.js + pnpm (via nvm recommended)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
nvm install 20
npm install -g pnpm
```

---

## 11.2 Project Structure Guide

### Top-Level Directory Purposes

| Directory | Contents | Language | Build System |
|-----------|---------|----------|-------------|
| **`crates/`** | Core Rust library crates (open-source) | Rust | Cargo workspace |
| **`ee/`** | Enterprise Edition crates + Admin UI | Rust + TS | Cargo + pnpm |
| **`clients/`** | IDE extensions & agent | TypeScript/Java/Kotlin | npm/Gradle |
| **`website/`** | Documentation site (Docusaurus) | MDX/React | Docusaurus |
| **`docker/`** | Docker images | Dockerfile | docker build |
| **`python/`** | Evaluation & load testing tools | Python | pip setup.py |
| **`experimental/`** | Research/experimental code | Various | varies |
| **`.changes/`** | changie unreleased fragments | YAML | .changes/ + changie |

### Workspace Hierarchy

```
tabby (workspace root)
├── Cargo.toml              # [workspace] definition + shared dependencies
├── pnpm-workspace.yaml     # TypeScript workspace config
├── turbo.json              # Turbo for parallel TS builds
│
├── crates/                  # Rust crates (14 total)
│   ├── tabby/               # Main server binary
│   ├── tabby-common/        # Shared types + config
│   ├── tabby-inference/     # Inference traits
│   ├── tabby-index/         # Tantivy index engine
│   ├── llama-cpp-server/    # llama.cpp integration
│   ├── http-api-bindings/   # HTTP model adapters
│   ├── ollama-api-bindings/ # Ollama adapter
│   ├── tabby-git/           # Git operations
│   ├── tabby-crawler/       # Web crawler
│   ├── tabby-download/      # Model download
│   ├── aim-downloader/      # Progress bar downloader
│   ├── hash-ids/            # ID obfuscation
│   ├── tabby-index-cli/     # Index CLI binary
│   └── sqlx-migrate-validate/  # Migration validation
│
├── ee/                      # Enterprise Edition (5 crates)
│   ├── tabby-webserver/     # Web server + service layer
│   ├── tabby-schema/        # GraphQL schema (Juniper)
│   ├── tabby-db/            # SQLite DB + 50 migrations
│   ├── tabby-db-macros/     # Proc macros
│   ├── tabby-ui/            # React Admin SPA (Vite)
│   └── tabby-email/         # Email templates
│
├── clients/                  # IDE & Agent
│   ├── tabby-agent/          # Core agent (TS)
│   ├── vscode/               # VS Code extension
│   ├── intellij/             # IntelliJ plugin
│   ├── vim/                  # Vim/Neovim client
│   ├── eclipse/              # Eclipse plugin
│   ├── tabby-chat-panel/     # Chat panel component lib
│   ├── tabby-threads/        # Thread management lib
│   ├── tabby-openapi/        # OpenAPI spec package
│   └── example-vscode-lsp/   # Example LSP client
│
├── website/                  # Documentation (Docusaurus)
│   ├── docs/                 # Markdown docs
│   ├── blog/                 # Blog posts
│   ├── src/                  # React components + config
│   └── static/               # Static assets
```

---

## 11.3 Build & Test

### Building

```bash
# From project root

# 1. Open-source server only
cargo build                          # Debug (slow)
cargo build --release                # Production optimized

# 2. With Enterprise Edition
cargo build --features ee            # Includes EE crates
cargo build --release --features ee

# GPU-specific builds
cargo build --release --features "ee cuda"     # CUDA GPU support
cargo build --release --features "ee rocm"     # AMD ROCm
cargo build --release --features "ee vulkan"   # Vulkan

# 3. TypeScript frontend (Admin UI)
cd ee/tabby-ui
pnpm install
pnpm build                         # Production build → dist/

# 4. All packages (CI equivalent)
pnpm build                         # Builds all TS packages in parallel (turbo)
```

### Testing

```bash
# Run all Rust tests
cargo test                          # All crates in workspace

# Test specific crate
cargo test -p tabby                 # Only crates/tabby/
cargo test -p tabby-db              # Only ee/tabby-db/

# EE database integration tests
cd ee/tabby-db
cargo test --all-features           # Includes SQLite connection tests

# TypeScript tests
cd clients/tabby-agent
pnpm test                           # Jest unit tests

# Web UI tests
cd ee/tabby-ui
pnpm test                           # Vitest + React Testing Library
```

### E2E Tests

```bash
# Playwright E2E tests (requires running server)
cargo build --release --features ee
./target/release/tabby serve &       # Start server in background

cd e2e                              # or wherever playwright config is
npx playwright test                  # Run browser-based E2E tests

# Clean up
pkill tabby
```

### CI Pipeline (`.github/workflows/`)

| Workflow | Trigger | 执行内容 |
|----------|---------|---------|
| `ci.yml` | Push / PR to main | cargo test + cargo build --features ee |
| `e2e.yml` | Push / PR | Playwright E2E tests (server + UI) |
| `docker-publish.yml` | Release tag | Publish Docker images to hub.docker.com |
| `release.yml` | Release tag | Semantic version release with changelog |
| `codegen.yml` | Manual trigger | Auto-generate GraphQL types, OpenAPI spec |

---

## 11.4 Code Style & Conventions

### Rust Conventions

```bash
# Format (required before PR)
cargo fmt --all

# Clippy (lint) — fix all warnings
cargo clippy --all-targets --all-features -- -D warnings

# Type checking without compilation
cargo check --all-targets --all-features
```

| 规则 | 标准 |
|------|------|
| Formatter | `rustfmt` (via `.rustfmt.toml`) |
| Linter | `clippy -- -D warnings` |
| Import order | `extern crate` → `use` (grouped by standard library / crates.io / own code) |
| Error handling | `anyhow::Result` for application errors; custom error types for public APIs |
| Logging | `tracing` crate (structured logging with spans/fields) |

### TypeScript Conventions

```bash
# Format + lint (pre-commit hooks recommended)
cd clients/tabby-agent
pnpm format        # Prettier
pnpm lint          # ESLint

# Type checking
pnpm typecheck     # tsc --noEmit
```

| 规则 | 标准 |
|------|------|
| Formatter | Prettier (`ee/tabby-ui/.prettierrc`) |
| Linter | ESLint (`@typescript-eslint/recommended`) |
| Type checking | `tsc --strict` |

---

## 11.5 Adding a New Model Provider

### Step-by-Step Guide

#### 1. Create adapter implementation

```rust
// crates/http-api-bindings/src/completion/new_provider.rs

use tabby_inference::CompletionStream;
use async_trait::async_trait;

#[derive(Clone)]
pub struct MyProvider {
    endpoint: String,
    api_key: Option<String>,
    model_name: String,
}

impl MyProvider {
    pub fn new(endpoint: String, api_key: Option<String>, model_name: String) -> Self {
        Self { endpoint, api_key, model_name }
    }
}

#[async_trait]
impl CompletionStream for MyProvider {
    async fn completion(&self, params: CompletionParams) -> Result<CompletionResponse> {
        // 1. Build HTTP request to the provider's API
        let body = serde_json::json!({
            "model": self.model_name,
            "prompt": params.prompt_template,
            "max_tokens": params.max_tokens,
        });

        let mut builder = reqwest::Client::new()
            .post(&self.endpoint)
            .header("Content-Type", "application/json");

        if let Some(ref key) = self.api_key {
            builder = builder.bearer_auth(key);
        }

        let resp = builder.json(&body).send().await?;
        let json: serde_json::Value = resp.json().await?;

        // 2. Parse response into tabby-inference types
        Ok(CompletionResponse {
            text: json["choices"][0]["text"].as_str().unwrap_or("").into(),
            finish_reason: None,
        })
    }
}
```

#### 2. Register the kind string in config parsing

```rust
// crates/tabby-common/src/config.rs — HttpModelConfig enum variant
impl ModelConfig {
    pub fn from_http_config(kind: &str, http_config: HttpModelConfig) -> Option<Self> {
        match kind {
            "llama.cpp/completion" => Some(Self::CompletionAdapter(...)),
            // ... existing kinds ...
            "my-provider/completion" => Some(Self::MyProvider(MyProvider::new(
                http_config.api_endpoint?,
                http_config.api_key,
                http_config.model_name?,
            ))),
            _ => None,
        }
    }
}
```

#### 3. Update documentation

In `MODEL_SPEC.md` or the website docs:
- Add model name to supported models list
- Document the `kind` string
- Provide prompt_template example

#### 4. Add tests

```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_my_provider_completion() {
        let provider = MyProvider::new(
            "http://localhost:9999".into(),
            None,
            "test-model".into(),
        );

        let response = provider.completion(CompletionParams {
            prompt_template: "test prompt".into(),
            max_tokens: 100,
            ..Default::default()
        }).await;

        assert!(response.is_ok());
    }
}
```

---

## 11.6 Adding a New Client Extension

### Scaffolding an IDE Plugin

```bash
# Clone and prepare extension directory
mkdir -p clients/my-ide-extension/{src,icons,package.json}

# package.json (for VS Code/NPM extensions)
cat > clients/my-ide-extension/package.json << 'EOF'
{
  "name": "tabby-myide",
  "displayName": "Tabby for MyIDE",
  "version": "0.1.0",
  "engines": { "vscode": "^1.85.0" },
  "main": "dist/extension.js",
  "activationEvents": ["onLanguage"],
  "contributes": {},
  "scripts": {
    "build": "tsc && webpack --config webpack.config.js",
    "watch": "tsc && webpack --watch"
  },
  "dependencies": { "@tabbyml/tabby-agent": "workspace:*" }
}
EOF
```

---

## 11.7 Database Migration Guide

### Creating a New Migration

```bash
# From ee/tabby-db/
cd ee/tabby-db

# Create a new migration file (using sqlx CLI)
sqlx migrate add add_new_table          # Creates migrations/YYYY-MM-DD_HHMMSS_add_new_table.sql
```

### Migration File Format

```sql
-- migrations/YYYY-MM-DD_HHMMSS_add_new_table.sql

-- up
CREATE TABLE IF NOT EXISTS _my_new_table (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- down
-- DROP TABLE _my_new_table;  -- (optional: rollback not always needed)
```

### Validation at Build Time

`sqlx-migrate-validate` crate ensures migration files match the Rust schema:

```bash
# This runs during `cargo build --features ee`:
# - Compares migration up scripts against current table schemas
# - Fails if there's a drift between code and migrations
```

### Best Practices

1. **Migrations are forward-only** — never modify existing migration files
2. **Use IF NOT EXISTS** for CREATE TABLE to handle re-runs gracefully
3. **Index columns that are frequently filtered** (e.g., `user_id`, `repo_url`)
4. **Encrypt sensitive data** (passwords, API keys) before inserting
5. **Add data migration scripts** inline if needed (ALTER TABLE + UPDATE in same file)

---

## 11.8 PR Review Process

### Changelog (.changes/)

Tabby uses [changie](https://changie.dev/) for managed changelog entries:

```bash
# Create a changelog fragment before submitting PR
npx changie new \
  --type feat \            # feat | fix | docs | refactor | chore | ...
  --component "tabby" \    # Which crate/module
  --details "Add support for new model type X"
```

This creates `.changes/unreleased/feat-tabby-YYYYMMDD-HHMMSS.yaml`:

```yaml
kind: feat
body: Add support for new model type X
time: 2025-08-12T00:00:00.000000000+00:00
custom:
  Component: tabby
```

### PR Checklist

- [ ] `cargo fmt --all` — Code formatted
- [ ] `cargo clippy` — No lint warnings
- [ ] `cargo test` — All tests pass
- [ ] Changelog fragment (if user-facing change)
- [ ] Updated docs (website/docs/) if behavior changed
- [ ] Added tests for new functionality

---

## 11.9 Issue Templates & Labels

### GitHub Issue Templates (`.github/ISSUE_TEMPLATE/`)

| Template | Purpose | When to use |
|----------|---------|-------------|
| `bug_report.md` | Bug report form | Any software defect / unexpected behavior |
| `feature_request.md` | Feature request | New functionality proposal |
| `config_bug_report.md` | Config parsing issue | TOML config errors, migration issues |

### Common Labels

| Label | Scope |
|-------|-------|
| `bug` | Software defect |
| `enhancement` | New feature / improvement |
| `documentation` | Docs improvements |
| `good first issue` | Beginner-friendly tasks |
| `help wanted` | Needs community contribution |
| `models` | Model provider / inference related |
| `indexing` | Tantivy index related |
| `ee` | Enterprise Edition specific |
| `client` | IDE extension related |

---

## 11.10 Versioning Strategy

Tabby follows **Semantic Versioning** (semver):

```
MAJOR.MINOR.PATCH-dev.N
 0   .  33  .  0   - dev.0
 │    │   │      └─ PR iteration within same dev version
 │    │   └──────── Patch: bug fixes, no API changes
 │    └──────────── Minor: new features (backward compatible)
 └───────────────── Major: breaking changes
```

| 变更类型 | Bump | 示例 |
|---------|------|------|
| Breaking API change | MAJOR | v0.32.0 → v1.0.0 |
| New feature, no breaking change | MINOR | v0.32.x → v0.33.0-dev.0 |
| Bug fix only | PATCH | v0.33.0-dev.0 → v0.33.0-dev.1 |
