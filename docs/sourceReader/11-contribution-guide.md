# 第十一章：贡献指南

## 11.1 Development Setup

### Required Toolchain

| 工具 | 版本 | 用途（依据） |
|------|---------|------|
| **Rust** | CI 固定 1.82.0（`env.RUST_TOOLCHAIN`，见 `.github/workflows/test-rust.yml`）；根 `Cargo.toml` 未声明 `rust-version` | Server backend compilation |
| **cargo** | 随 Rust 安装 | Rust package manager & build tool |
| **Node.js** | CI 统一用 18（`.github/workflows/test-pnpm.yml`、`.github/workflows/autofix-pnpm.yml`、`.github/workflows/gh-pages.yml`） | `ee/tabby-ui` 与 `clients/*` 构建 |
| **pnpm** | CI 用 `pnpm/action-setup@v4` + `version: 9` | TypeScript monorepo package manager |
| **protoc** | apt 装 `protobuf-compiler`；CentOS 分支下载 protoc 3.15.8 | `ci/prepare_build_environment.sh` 列出的构建依赖 |
| **SQLite3 + graphviz** | 发行版自带即可 | `make update-db-schema` 导出 `ee/tabby-db/schema/schema.sql` 与 `schema.svg` |
| **mailpit** | 本地 SMTP | EE 邮件相关测试需要（同一脚本 `install_mailpit`） |
| **git submodule** | `git clone --recurse-submodules` | `crates/llama-cpp-server/llama.cpp`（见 `.gitmodules`） |

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

> CI 在每次构建前执行的就是这份依赖清单：`ci/prepare_build_environment.sh`（macOS 分支 `brew install protobuf` + mailpit；Debian/Ubuntu 分支 `apt-get install protobuf-compiler libopenblas-dev sqlite3 graphviz libcurl4-openssl-dev`；无 apt 时走 CentOS/yum 分支），Windows 对应 `ci/prepare_build_environment.ps1`。本地环境直接 `sudo bash ./ci/prepare_build_environment.sh` 与 CI 对齐最省事。

---

## 11.2 Project Structure Guide

### Top-Level Directory Purposes

| Directory | Contents | Language | Build System |
|-----------|---------|----------|-------------|
| **`crates/`** | Core Rust library crates (open-source) | Rust | Cargo workspace |
| **`ee/`** | Enterprise Edition crates + Admin UI | Rust + TS | Cargo + pnpm |
| **`clients/`** | IDE extensions & agent | TypeScript/Java/Kotlin | npm/Gradle |
| **`website/`** | Documentation site (Docusaurus) | MDX/React | Docusaurus（站内用 `yarn`，见 `CONTRIBUTING.md`） |
| **`docker/`** | Docker images | Dockerfile | `docker/Dockerfile.cuda`（CI 按 device-type 矩阵构建） |
| **`python/`** | `python/tabby/trainer.py`、`python/tabby-eval/modal/`、`python/tabby-loadtest/` | Python | 没有统一打包入口，只有自动生成的 `python/tabby-eval/modal/tabby_python_client/setup.py` |
| **`experimental/`** | Research/experimental code | Various | varies |
| **`ci/`** | 构建环境准备脚本 + 上游打包脚本 | Shell/PowerShell | `ci/prepare_build_environment.sh`、`ci/prepare_build_environment.ps1`、`ci/package-from-upstream.sh` |
| **`.github/`** | CI workflow 与 issue 模板 | YAML/Markdown | 16 个 workflow（`.github/workflows/`）+ 2 个 issue 模板 |
| **`.changes/`** | changie unreleased fragments + 历史版本 | YAML | `.changie.yaml` + changie |
| **`rules/`** | ast-grep 规则目录 | YAML | 由 `sgconfig.yml` 指向，`.github/workflows/ast-grep-lint.yml` 执行 |

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
├── ee/                      # Enterprise Edition (4 个 Rust crate + 2 个非 cargo 工程)
│   ├── tabby-webserver/     # Web server + service layer
│   ├── tabby-schema/        # GraphQL schema (Juniper)
│   ├── tabby-db/            # SQLite DB + 49 个 up 迁移（migrations/ 共 98 个 .sql）
│   ├── tabby-db-macros/     # Proc macros
│   ├── tabby-ui/            # Admin UI：Next 13 + App Router（ee/tabby-ui/next.config.js），非 cargo crate
│   └── tabby-email/         # Email templates（非 cargo crate）
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

# 1. 默认就带 EE：crates/tabby/Cargo.toml 的 default = ["ee", "llama-cpp-server/binary"]
cargo build                          # Debug（slow，已含 ee）
cargo build --release                # Production optimized

# 2. 社区版（open-source only）= 关掉 default features
cargo build --no-default-features                    # 同时会去掉 ee 与 llama-cpp-server/binary
cargo build --no-default-features --features llama-cpp-server/binary   # 只去 ee，保留本地 llama-server

# GPU-specific builds：cuda / rocm / vulkan 三个 feature 直接透传给 llama-cpp-server
cargo build --release --features cuda     # CUDA GPU support
cargo build --release --features rocm     # AMD ROCm
cargo build --release --features vulkan   # Vulkan
# Metal (Apple Silicon) 不需要额外 feature，见 CONTRIBUTING.md 的 cargo run serve --device metal

# 3. TypeScript frontend (Admin UI)：ee/tabby-ui 是 Next 应用，ee/tabby-ui/next.config.js 里 output = "export"
cd ee/tabby-ui
pnpm install
pnpm build                         # next build → 静态产物 out/，再由 make update-ui 拷进 ee/tabby-webserver/ui

# 4. All packages (CI equivalent)：根 package.json 的 build = turbo build
pnpm build                         # Builds all TS packages in parallel (turbo)
```

### Testing

```bash
# Run all Rust tests —— CONTRIBUTING.md 建议本地先跳过 golden 用例（它们会真的加载模型，很慢）
cargo test -- --skip golden

# Test specific crate
cargo test -p tabby                 # Only crates/tabby/
cargo test -p tabby-db              # Only ee/tabby-db/（迁移一致性用例见 ee/tabby-db/src/migration_tests.rs）

# 与 CI 完全对齐的三条命令（.github/workflows/test-rust.yml 的 tests job 就是这三条）
cargo test --doc
cargo test --bin tabby --no-default-features   # community build
cargo test --bin tabby --lib

# TypeScript tests
cd clients/tabby-agent
pnpm test                           # mocha（clients/tabby-agent/.mocharc.js：spec = src/**/*.test.ts）

# Web UI tests
cd ee/tabby-ui
pnpm test                           # vitest run，用例在 ee/tabby-ui/test/
```

> EE 的邮件相关用例需要本地 SMTP：CI 与本地都靠 `ci/prepare_build_environment.sh` 里的 `install_mailpit` 起 mailpit。

### E2E Tests

仓库里没有独立的 Playwright 端到端工程：根目录下没有 `e2e/` 目录，`.github/workflows/` 的 16 个 workflow 里也没有 e2e job。端到端回归由 **golden tests** 承担，它们是普通 `cargo test`，自己拉起 server：

```bash
# 1. 先产出 target/debug/tabby —— goldentests.rs 直接 spawn 这个路径
cargo build

# 2. CPU 变体在所有平台都会跑；metal 变体被 cfg(target_os = "macos", target_arch = "aarch64") 限定
cargo test run_golden_tests_cpu
cargo test run_chat_golden_tests_cpu

# 3. 跑全部 golden（含 chat 与平台限定的 metal 变体）
cargo test --test goldentests --test goldentests_chat
```

`crates/tabby/tests/goldentests.rs` · `initialize_server` `+2` 起的是 `tabby serve --model TabbyML/StarCoder-1B --no-webserver --port 9090`，等 `http://127.0.0.1:9090/v1/health` 就绪后向 `/v1/completions` 发请求，结果用 `insta::assert_yaml_snapshot!` 与 `crates/tabby/tests/snapshots/` 里的 `.snap` 比对；`crates/tabby/tests/goldentests_chat.rs` 同理跑 chat。

负载测试是另一套：`python/tabby-loadtest/run.sh` 用 Modal 部署 server，再用 `k6 run loadtest.js` 做二分搜索求最大并发，结果写 `python/tabby-loadtest/record.csv`。

本地一把梭起全部进程可用 `make dev`（实际执行 `tmuxinator start -p .tmuxinator/tabby.yml`）。

### CI Pipeline (`.github/workflows/`)

实测 16 个 workflow 文件，**没有** `ci.yml` 这种「一条总管流水线」，也没有 e2e / codegen workflow：Rust 与 TS 各自一个 test workflow，自动修复（autofix）与 lint 独立成流，发布按「二进制 / 镜像 / 各 IDE 插件」拆开。逐个打开文件核对后的清单：

| Workflow | Trigger | 执行内容 |
|----------|---------|---------|
| `.github/workflows/test-rust.yml` | push main、PR→main（paths：`Cargo.toml`、`Cargo.lock`、`crates/**`、`ee/**`，排除 `ee/tabby-ui/**`） | `tests` job（仅 PR 事件）：`cargo test --doc` → `cargo test --bin tabby --no-default-features`（community build）→ `cargo test --bin tabby --lib`；`coverage` job：`cargo llvm-cov --bin tabby --lib --lcov` 后上传 Codecov |
| `.github/workflows/test-pnpm.yml` | PR→main（paths：`clients/**`、`ee/tabby-ui/**`） | `pnpm install` → `pnpm lint` → `pnpm test` → `pnpm build`（Node 18 + pnpm 9） |
| `.github/workflows/test-intellij.yml` | PR→main（paths：`clients/tabby-agent/**`、`clients/intellij/**`） | JDK 17 (zulu) + Node 18 下构建并测试 IntelliJ 插件 |
| `.github/workflows/autofix-rust.yml` | PR→main（paths：`Cargo.toml`、`Cargo.lock`、`crates/**`、`ee/**`） | nightly 工具链跑 `make fix`（`cargo machete` + `cargo +nightly fmt` + `cargo clippy --fix`）与 `make update-graphql-schema`，由 autofix-ci 把改动推回 PR |
| `.github/workflows/autofix-pnpm.yml` | PR→main（paths：`clients/**`、`ee/tabby-ui/**`） | `pnpm lint:fix` 后由 autofix-ci 回推提交 |
| `.github/workflows/autofix-python.yml` | PR→main（paths：`python/**`） | `chartboost/ruff-action@v1` 对 `./python` 执行 `--fix` |
| `.github/workflows/ast-grep-lint.yml` | `[push]`（任何分支） | `ast-grep/action@v1.4`（0.20.1）按 `sgconfig.yml` 指向的 `rules/` 目录做模式 lint |
| `.github/workflows/bloat.yml` | PR→main + push main | stable 工具链上 `wsxiaoys/cargo-bloat-action`，`include_packages: tabby`，对比二进制体积并在 PR 上留评 |
| `.github/workflows/release.yml` | tag `v*` / `nightly`（排除 `*-dev.*`、`vscode@*`、`vim@*`）、`workflow_dispatch`、PR（仅改本文件时） | 4 个 job：`release-llama-server-binary`、`release-binary`（跨 target 矩阵 `cargo build --release --package tabby`）、`package-from-upstream`、`pre-release`（`ncipollo/release-action` 创建 GitHub Release） |
| `.github/workflows/docker.yml` | tag `v*`、cron `0 20 */1 * *`、`workflow_dispatch` | device-type 矩阵（当前只有 `cuda`）用 `docker/Dockerfile.cuda` build & push 到 `ghcr.io/tabbyml/tabby` 与 Docker Hub `tabbyml/tabby`；发布 tag 时额外产出 `-cuda11` 后缀镜像并更新 Docker Hub 描述 |
| `.github/workflows/nightly.yml` | cron `0 20 */1 * *`、`workflow_dispatch` | `EndBug/latest-tag` 把 `nightly` tag 移到最新提交（`release.yml` 的 push tags 里包含 `nightly`） |
| `.github/workflows/release-vscode.yml` | tag `vscode@*`、`workflow_dispatch` | `cd clients/vscode && pnpm run $(node scripts/publish.cjs)`，即 vsce 打包发布 |
| `.github/workflows/release-vim.yml` | tag `vim@*`、`workflow_dispatch` | 把 `clients/vim/` 内容同步进独立仓库 `TabbyML/vim-tabby` 并发 release |
| `.github/workflows/release-intellij.yml` | tag `intellij@*`、`workflow_dispatch` | JDK 17 + Node 18 构建插件并发布到 JetBrains Marketplace |
| `.github/workflows/gh-pages.yml` | push main（paths：`website/**`）、`workflow_dispatch` | 构建 `website/`（Docusaurus）并部署到 GitHub Pages |
| `.github/workflows/gpt-translate.yml` | `issue_comment` created | 评论体含 `/gpt-translate` 或 `/gt` 时调用 `3ru/gpt-translate` 翻译 issue/PR |

注：`make update-graphql-schema` 的真实命令是 `cargo run --package tabby-schema --example update-schema --features=schema-language`；`make update-ui` 会把 `ee/tabby-ui/out` 拷成 `ee/tabby-webserver/ui`、`ee/tabby-email/out` 拷成 `ee/tabby-webserver/email_templates`——前端产物是随 Rust 二进制发布的，因此改完 EE 前端要跑这两条而不是等 CI 生成。GraphQL/OpenAPI 类型的生成没有独立 workflow，就挂在 `Makefile` 目标上（`update-openapi-doc`、`update-db-schema`）。

---

## 11.4 Code Style & Conventions

### Rust Conventions

```bash
# 与 CI 完全等价的一条：Makefile 的 fix 目标
make fix          # = cargo machete --fix; cargo +nightly fmt; cargo clippy --fix --allow-dirty --allow-staged

# 拆开跑（只检查、不改文件）
cargo +nightly fmt --all -- --check

# 依赖是否有多余项
cargo machete
```

| 规则 | 标准 |
|------|------|
| Formatter | `rustfmt`（配置 `.rustfmt.toml`：`reorder_imports = true`、`imports_granularity = "crate"`、`group_imports = "StdExternalCrate"`） |
| Import order | 三段：标准库 → 外部 crate → 本仓库 crate，同一 crate 合并成一条 `use`（`crate` 粒度） |
| Lint | `cargo clippy --fix`；`.github/workflows/autofix-rust.yml` 在 PR 上自动回推修复结果，CI 里**没有** `clippy -- -D warnings` 这道硬门 |
| 模式约束 | `sgconfig.yml` + `rules/` 的 ast-grep 规则，由 `.github/workflows/ast-grep-lint.yml` 执行。例：`rules/use-schema-result.yml`（EE 对外接口要用 `schema::Result`，见 `ee/tabby-schema/src/schema/mod.rs` · `Result` `+0`）、`rules/do-not-use-logkit-crate.yml`（不要 `use logkit::…`）、`rules/only-dao-and-policy-can-depend-tabby-db.yml`（只有 dao/policy 能依赖 tabby-db） |
| Error handling | 应用层 `anyhow::Result`（如 `crates/tabby/src/services/code.rs`）；EE 对外接口按 `use-schema-result` 规则返回 `schema::Result` |
| Logging | `tracing` crate（结构化 spans/fields）；`logkit` 只留给 background job 日志，规则强制 |

### TypeScript Conventions

```bash
# 与 CI 等价的：根 package.json 的脚本全是 turbo 转发
pnpm lint          # turbo lint -> 各包自己的 lint
pnpm lint:fix      # turbo lint:fix（autofix-pnpm.yml 跑的就是这条）
pnpm test          # turbo test
pnpm build         # turbo build

# 单包内可用 scripts（见 clients/tabby-agent/package.json、ee/tabby-ui/package.json）
cd ee/tabby-ui && pnpm type-check   # tsc --noEmit
cd clients/tabby-agent && pnpm test # mocha
```

| 规则 | 标准 |
|------|------|
| Formatter | Prettier（`clients/tabby-agent/.prettierrc.json`、`ee/tabby-ui/prettier.config.cjs`） |
| Linter | ESLint（`clients/tabby-agent/.eslintrc` extends `eslint:recommended` + `plugin:@typescript-eslint/recommended` + `prettier`；`ee/tabby-ui` 走 `next lint`） |
| Type checking | 两个 tsconfig 都开 `strict: true`；tabby-ui 提供 `pnpm type-check`，tabby-agent 无独立 typecheck 脚本 |

---

## 11.5 Adding a New Model Provider

### Step-by-Step Guide

#### 1. Create adapter implementation

适配层**没有** `new_provider.rs` 这类模板文件，也**没有**注册表：`crates/http-api-bindings/src/` 的真实清单是 `lib.rs`、`rate_limit.rs`、`completion/{mod,llama,mistral,openai}.rs`、`chat/mod.rs`、`embedding/{mod,azure,llama,mistral,openai}.rs`。新增一个 provider = 新增一个模块文件 + 在对应 `mod.rs` 的 `create()` 里加一个 match 分支。

```rust
// crates/http-api-bindings/src/completion/<provider>.rs（尖括号是占位符：按 provider 取名，如 openai.rs）
use async_trait::async_trait;
use tabby_inference::{CompletionOptions, CompletionStream};

#[derive(Clone)]
pub struct MyProvider {
    endpoint: String,
    api_key: Option<String>,
    model_name: String,
}

#[async_trait]
impl CompletionStream for MyProvider {
    // trait 里唯一必须实现的是 `generate`，返回 `BoxStream<'life0, String>`；
    // `generate_sync` 有默认实现（把流聚合成一个 String）。
    // 现有三个 engine 都不返回 Result，HTTP 错误在内部 log 后产出空流。
    async fn generate(
        &self,
        prompt: &str,
        options: CompletionOptions,
    ) -> BoxStream<'life0, String> {
        // 1. Build HTTP request to the provider's API
        // 2. 用 async_stream::stream! / futures::StreamExt 把 SSE 或整包响应转成 Stream
        todo!()
    }
}
```

> 抄写法时以 `crates/http-api-bindings/src/completion/mistral.rs` 为最接近的样板：它是「非 OpenAI 形状」的自建请求（endpoint / api_key / model_name 三元组 + 一个 `FIMRequest`），与 `crates/http-api-bindings/src/completion/openai.rs`（兼容端点 + `OPENAI_LEGACY_COMPLETION_FIM_ALIASES` 三个别名共用一个 engine）刚好覆盖两种形态。

#### 2. Register the kind in the dispatcher

kind 字符串**不在** `crates/tabby-common/src/config.rs` 里枚举 —— `HttpModelConfig.kind` 就是一个裸 `String`，必填、没有 `#[serde(default)]`，字段文档只说明它对应 completion / chat / embedding 三类模型。分发完全靠 `http-api-bindings` 里的 `match config.kind.as_str()`：

```rust
// crates/http-api-bindings/src/completion/mod.rs · create()（节选：各 engine 的参数用 /* ... */ 省略）
pub async fn create(model: &HttpModelConfig) -> Arc<dyn CompletionStream> {
    let engine = match model.kind.as_str() {
        "llama.cpp/completion" => LlamaCppEngine::create(/* ... */),
        "ollama/completion" => ollama_api_bindings::create_completion(model).await,
        "mistral/completion" => MistralFIMEngine::create(/* ... */),
        x if OPENAI_LEGACY_COMPLETION_FIM_ALIASES.contains(&x) => {
            OpenAICompletionEngine::create(/* ... */, true)
        }
        "openai/legacy_completion_no_fim" | "vllm/completion" => {
            OpenAICompletionEngine::create(/* ... */, false)
        }
        "my-provider/completion" => MyProvider::create(model).await, // ← 新分支加在这里
        unsupported_kind => panic!("Unsupported model kind for http completion: {unsupported_kind}"),
    };
    Arc::new(rate_limit::new_completion(engine, model.rate_limit.request_per_minute))
}
```

两个必须知道的后果：**未命中的 kind 走的是 `panic!` 而不是返回错误**（配置里打错 kind 会让 serve 在装载模型时直接崩，`crates/http-api-bindings/src/embedding/mod.rs` 同样是 `panic!`）；以及**出口只有一条**，所有 engine 最后都被 `rate_limit::new_*` 包一层，新分支不要自己再包限流。
另外同一文件的 `build_completion_prompt()` 按 kind 决定用内置 FIM 模板 `"{prefix}<|FIM|>{suffix}"` 还是用户配的 `prompt_template`，新 provider 若走 FIM 约定要一并登记。
上游调用点两处：`crates/tabby/src/services/model/mod.rs` 与 `crates/llama-cpp-server/src/lib.rs`（后者同样会转调 http-api-bindings）。

#### 3. Update documentation

- `MODEL_SPEC.md`（仓库根）只讲**本地 GGUF 模型的落盘规范**两节：`tabby.json`（模型元数据，下载时由 `crates/tabby-common/src/registry.rs · save_model_info()` 写到 `~/.tabby/models/<registry>/<name>/tabby.json`，运行时由 `crates/llama-cpp-server/src/lib.rs · PromptInfo::read()` 读出 prompt/chat template）与 `ggml/`（权重目录，分片文件以 `model-00001-of-` 前缀为入口，见 `GGML_MODEL_PARTITIONED_PREFIX`）。远程 provider 的接入说明**不写在这里**。
- 远程 provider 的文档落点是 `website/docs/references/models-http-api/`（每个 provider 一个 `.md`，共 19 篇），以及 `website/docs/references/cloud-deployment/`；`kind` 字符串与 `prompt_template` 示例写进对应那篇。

#### 4. Add tests

本 crate 现有测试只有三处，照它们的形态加：

```rust
// crates/http-api-bindings/src/completion/mod.rs · tests —— 纯函数用 #[test]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_fim_prompt_no_fim() { /* (输入, 期望) 元组表逐条 assert_eq! */ }
}
```

```rust
// crates/http-api-bindings/src/embedding/openai.rs · tests —— 需要 async 引擎的用 #[tokio::test]
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_xxx() { /* ... */ }
}
```

> 注意 `completion/` 目录下**没有**逐 engine 的 `#[tokio::test]`（`llama.rs`/`mistral.rs`/`openai.rs` 三个 completion adapter 都没带测试），要测新 adapter 属于「开先例」，不是照抄现成样板 —— 现成样板在 `embedding/llama.rs` 与 `embedding/openai.rs`。

---

## 11.6 Adding a New Client Extension

### Scaffolding an IDE Plugin

下面是一份**假想示例**：`clients/` 下现有 9 个客户端目录（`tabby-agent`、`vscode`、`intellij`、`vim`、`eclipse`、`tabby-chat-panel`、`tabby-threads`、`tabby-openapi`、`example-vscode-lsp`，其中 `example-vscode-lsp` 只有一个 `README.md`，不是可复制的脚手架），仓库里并没有 `clients/my-ide-extension/` 这个目录，示例里的字段全部照 `clients/vscode/package.json` 的真实取值对齐：

```bash
# 示例目录（假想路径，仓库中不存在）：clients/my-ide-extension/
mkdir -p clients/my-ide-extension/src

# package.json —— 示例内容（目标目录在仓库中不存在，需要你自己创建），字段形态取自 clients/vscode/package.json
cat > clients/my-ide-extension/package.json << 'EOF'   # 目标目录在仓库中不存在，需你自己创建
{
  "name": "tabby-myide",
  "displayName": "Tabby for MyIDE",
  "version": "0.1.0",
  "engines": { "vscode": "^1.82.0" },
  "main": "./dist/node/extension.js",
  "browser": "./dist/browser/extension.js",
  "activationEvents": ["onLanguage"],
  "contributes": {},
  "scripts": {
    "build": "tsc -p ./tsconfig.build.json --noEmit && tsup --minify",
    "watch": "tsup --watch"
  },
  "devDependencies": { "tabby-agent": "workspace:*" }
}
EOF
```

三条实测约束：

1. **包名是 `tabby-agent`，不是 `@tabbyml/tabby-agent`**（`clients/tabby-agent/package.json` 的 `name`），且 `clients/vscode/package.json` 把它放在 `devDependencies` 里用 `"workspace:*"` 引用，打包时 `vsce package --no-dependencies` 不带运行时依赖。
2. **构建器是 tsup，不是 webpack**：`clients/vscode/package.json` · `build` 是 `tsc -p ./tsconfig.build.json --noEmit && tsup --minify`，产物按平台分成 `dist/node/`、`dist/browser/`（`watch --platform browser`），所以 `main` / `browser` 指到 `dist/<platform>/extension.js`。
3. **新目录会被 workspace 自动收进来**：`pnpm-workspace.yaml` 的 `packages` 只有三条 glob —— `clients/*`、`ee/tabby-ui`、`ee/tabby-email`；放进去后 `pnpm build` / `pnpm lint`（都是 `turbo` 转发）与 `.github/workflows/test-pnpm.yml`（paths 含 `clients/**`）就都会覆盖到它。发布则要另加一条类似 `.github/workflows/release-vscode.yml` 的 workflow（它靠 tag `vscode@*` 触发，执行 `cd clients/vscode && pnpm run $(node scripts/publish.cjs)`）。

---

## 11.7 Database Migration Guide

### Creating a New Migration

迁移不是 `sqlx migrate add` 生成的时间戳文件，而是**手工成对编号**的 `.up.sql` / `.down.sql`：`ee/tabby-db/migrations/` 实测 49 对（98 个 `.sql`），编号从 `0001_registration-token-table` 到 `0050_add-repository-refs`，其中 **0032 是历史缺号**（所以最大号 0050 对应 49 个文件）。

```bash
cd ee/tabby-db
ls migrations/*.up.sql | tail -1                        # 看当前最大号（0050）
# 新迁移取最大号 +1，手写 up/down 两个文件（sqlx CLI 的时间戳命名不要用，会和现有序号体系打架）
$EDITOR migrations/0051_add_my_new_table.up.sql          # 示例文件名，仓库中不存在
$EDITOR migrations/0051_add_my_new_table.down.sql        # 示例文件名，仓库中不存在
```

### Migration File Format

up 与 down 是**两个独立文件**，不是一个文件里两段注释（以最新的 `ee/tabby-db/migrations/0050_add-repository-refs.up.sql` 为样板）：

```sql
-- migrations/0051_add_my_new_table.up.sql（示例文件名，仓库中不存在）
ALTER TABLE repositories ADD COLUMN refs TEXT;
```

```sql
-- migrations/0051_add_my_new_table.down.sql（示例文件名，仓库中只到 0050）（示例文件名，仓库中不存在）
ALTER TABLE repositories DROP COLUMN refs;
```

建表类迁移的风格是 `CREATE TABLE`（49 个 up 里有 29 个出现 `CREATE TABLE`，其中只有 11 个写成 `IF NOT EXISTS`，所以「一律用 IF NOT EXISTS」并不是现状）。改完库表后同步 schema 快照与可视化：`make update-db-schema`（内部跑 `sqlite3 ee/tabby-db/schema.sqlite ".schema --indent"` 与 `dot -Tsvg`）。

### 启动时的迁移校验（不是构建期）

`crates/sqlx-migrate-validate` 提供的是 `Validate` trait（`crates/sqlx-migrate-validate/src/validate.rs` · `Validate` `+0`）。它的 `validate()` 并不比对表结构，只做四件事，全部围绕「库里已应用的迁移记录 vs 编译进二进制的迁移源」：

1. `dirty_version()` 非空 → `MigrateError::Dirty`；
2. `validate_applied_migrations` `+0` → 数据库里有源码中不存在的 version → `MigrateError::VersionMissing`（`ignore_missing = true` 时跳过这一步，默认是 `false`）；
3. 遍历源码里的 up 迁移：已应用的比对 checksum，不一致 → `MigrateError::VersionMismatch`；从没应用过 → `ValidateError::VersionNotApplied`（错误枚举见 `crates/sqlx-migrate-validate/src/error.rs` · `ValidateError` `+0`）；
4. 若开了锁则 `unlock()`，让后续真正的 migrate 能跑。

调用点在 `ee/tabby-db/src/lib.rs` · `backup_db` `+4`：`sqlx::migrate!("./migrations").validate(&mut *conn)` 返回 `Ok` 意味着「没有待应用的迁移」，于是直接跳过备份；返回 `Err` 就先把 db 文件复制成 `*.backup-YYYYMMDD.sqlite`（prod 是 `db.backup-*`，非 prod 是 `dev-db.backup-*`），随后 `init_db` `+1` 用 `.run(&pool)` 应用迁移。构建期唯一发生的事情是 `sqlx::migrate!` 这个宏把 `ee/tabby-db/migrations/` 嵌进二进制——校验时机在启动，不在 `cargo build`。

### Best Practices

1. **成对写 down** —— 49 个 up 都有对应的 down 文件，`0050` 的 down 就是 `ALTER TABLE ... DROP COLUMN ...`
2. **不要改历史迁移** —— 已应用的 version/checksum 一旦被改，`validate()` 直接 `VersionMismatch`，启动前先备份但迁移会失败
3. **Index columns that are frequently filtered**（e.g., `user_id`, `repo_url`）
4. **Encrypt sensitive data** (passwords, API keys) before inserting
5. **数据迁移与 DDL 写在同一个 up 里**（ALTER + UPDATE 同文件），因为 sqlx 只认 version 顺序执行

---

## 11.8 PR Review Process

### Changelog (.changes/)

Tabby uses [changie](https://changie.dev/) for managed changelog entries（`CONTRIBUTING.md` 末尾「Changelog」一节原话是「use `changie new` command」）：

```bash
# 提交 PR 前生成一个未发布片段（交互式：先从下面三个 kind 里选，再填 body）
npx changie new
```

kind 的合法值只有 `.changie.yaml` 里 `kinds:` 声明的三个，且各自绑定了自动升版位阶——**没有** `feat|fix|docs|refactor|chore` 这套，也**没有** `--component` 这种自定义字段（`.changie.yaml` 未声明 `custom`）：

| Kind | `auto` | 落到 CHANGELOG 的分组标题 |
|------|--------|--------------------------|
| `Notice` | minor | `### Notice` |
| `Features` | minor | `### Features` |
| `Fixed and Improvements` | patch | `### Fixed and Improvements` |

片段写进 `.changes/unreleased/`（本次实测该目录为空，具体文件名由 changie 按时间戳生成），字段是 `kind` / `body` / `time`。发版时 changie 按 `changesDir: .changes`、`changelogPath: CHANGELOG.md`、`versionFormat: '## {{.Version}} ({{.Time.Format "2006-01-02"}})'`、`kindFormat: '### {{.Kind}}'`、`changeFormat: '* {{.Body}}'` 把它们汇总成 `CHANGELOG.md`，历史版本已经落在 `.changes/v0.11.0.md` … 这些文件里。

### PR Checklist

- [ ] `make fix` — 格式化 + clippy 自动修复 + `cargo machete`（`.github/workflows/autofix-rust.yml` 跑的就是这条）
- [ ] `pnpm lint` / `pnpm lint:fix` — TS 侧（`.github/workflows/autofix-pnpm.yml`）
- [ ] `cargo test -- --skip golden` — 除 golden 之外的全部 Rust 用例
- [ ] Changelog fragment (if user-facing change)
- [ ] Updated docs (website/docs/) if behavior changed
- [ ] Added tests for new functionality
- [ ] 若改了 EE 前端/邮件模板：`make update-ui`；若改了 GraphQL schema：`make update-graphql-schema`（autofix 会代劳，但本地先跑更省事）

---

## 11.9 Issue Templates & Labels

### GitHub Issue Templates (`.github/ISSUE_TEMPLATE/`)

| Template | Purpose | When to use |
|----------|---------|-------------|
| `bug_report.md` | Bug report form | Any software defect / unexpected behavior |
| `feature_request.md` | Feature request | New functionality proposal |

`.github/ISSUE_TEMPLATE/` 只有上面这两个文件 —— 没有 `config_bug_report.md`。配置解析类问题走普通 `bug_report.md`（复现步骤里贴 `config.toml` 片段）。

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

这张表在源码里能查到的只有两个：`.github/ISSUE_TEMPLATE/bug_report.md` 的 frontmatter 写 `labels: bug-unconfirmed`（新报的 bug 先标「未确认」，而不是直接 `bug`），`.github/ISSUE_TEMPLATE/feature_request.md` 写 `labels: enhancement`。除此之外仓库里没有 `.github/labels.yml` 一类的标签定义文件，其余标签属于 GitHub 仓库侧配置，只能到 issue 页面上核对（`CONTRIBUTING.md` 里给出的「挑 issue」链接用的就是 `good first issue` + `no:assignee` 这个过滤式）。

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

三点实测补充：

1. 版本号的真身在根 `Cargo.toml` 的 workspace `version = "0.33.0-dev.0"`；`.github/workflows/release.yml` 与 `.github/workflows/docker.yml` 的 tag 触发都显式排除了 `*-dev.*`，所以 dev 版**不会**产出正式 release 与镜像。
2. MINOR / PATCH 不是人拍脑袋，而是 `.changie.yaml` 里每个 kind 的 `auto` 字段：`Features` 与 `Notice` → minor，`Fixed and Improvements` → patch。
3. 批量改版本号走 `Makefile` 的 `bump-version`（`cargo ws version --force "*" --no-individual-tags --allow-branch "main"`），发版分支上是 `bump-release-version`（`--allow-branch "r*"`）。
