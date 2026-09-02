# 第三章：核心 Crates（服务端开源部分）

## 概述

Tabby 的 Rust workspace 共包含 **14 个 crates**，分为以下功能域：

```
crates/
├── tabby                     # Server binary + routes + services
├── tabby-common              # Shared config types & abstractions
├── tabby-inference           # Inference trait definitions
├── llama-cpp-server          # llama.cpp process management
├── http-api-bindings         # HTTP model adapter (OpenAI/Mistral/etc)
├── ollama-api-bindings       # Ollama-specific adapters
├── tabby-index               # Tantivy code + doc indexing
├── tabby-index-cli           # Index CLI tool
├── tabby-git                 # Git repository operations
├── tabby-crawler             # Web content crawler
├── tabby-download            # Model download utility
├── aim-downloader            # Progress bar downloader (internal)
├── hash-ids                  # ID obfuscation library
└── sqlx-migrate-validate     # Migration validation helper
```

---

## 3.1 crates/tabby — Server Binary

**职责**: Tabby 的主入口和 API server，包含 CLI、HTTP routes、业务 service。

### 目录结构

```
crates/tabby/src/
├── main.rs          # CLI entry point (subcommands: serve / download)
├── serve.rs         # Server startup, Axum router assembly, OpenAPI merge
├── download.rs      # Model download logic
├── routes/          # HTTP API route handlers
│   ├── completions.rs  # POST /v1/completions
│   ├── chat.rs         # POST /v1/chat/completions
│   ├── events.rs       # POST /v1/events
│   ├── health.rs       # GET  /v1/health
│   ├── metrics.rs      # GET  /metrics
│   ├── models.rs       # GET  /v1/models
│   └── server_setting.rs # GET /v1/server/settings
└── services/        # Business logic
    ├── completion/      # Code completion service + prompt builder
    │   ├── postprocess/ # (shared with client-side rules)
    ├── embedding.rs     # Embedding model integration
    ├── tantivy.rs       # Tantivy index management
    ├── structured_doc/  # Structured document search (Serper + Tantivy)
    ├── code.rs          # Code search service
    ├── event.rs         # Event logging service
    └── model/mod.rs     # Model download & loading
```

### Key Subcommands

```bash
# CLI entry point: main.rs parses subcommands
tabby serve      # Start the API server (default)
tabby download   # Download model weights to ~/.tabby/models/
```

### Feature Flags

| Feature | 效果 |
|---------|------|
| `cuda` | 启用 NVIDIA CUDA GPU backend |
| `rocm` | 启用 AMD ROCm GPU backend |
| `vulkan` | 启用 Vulkan GPU backend |
| `ee` | 合并 EE webserver routes + types |

---

## 3.2 crates/tabby-common — Shared Types

**职责**: 为所有 crate 提供共享的配置结构体、API 类型定义和路径工具。

### 核心配置结构 (`config.rs`)

```rust
// Config is the top-level config struct loaded from ~/.tabby/config.toml
pub struct Config {
    pub repositories: Vec<RepositoryConfig>,   // Git repos to index
    pub server: ServerConfig,                   // Server timeout settings
    pub model: ModelConfigGroup,                // Completion/Chat/Embedding models
    pub completion: CompletionConfig,           // Completion-specific params
    pub embedding: EmbeddingConfig,             // Embedding model config
    pub answer: AnswerConfig,                   // Answer Engine search params
    pub additional_languages: Vec<Language>,    // Custom language definitions
}

pub struct ModelConfigGroup {
    pub completion: Option<ModelConfig>,   // Completion model (required)
    pub chat: Option<ModelConfig>,          // Chat model (optional → Answer Engine)
    pub embedding: ModelConfig,             // Embedding model (defaults to Nomic-Embed-Text)
}

// ModelConfig enum — two variants
pub enum ModelConfig {
    Http(HttpModelConfig),   // Remote HTTP API backend
    Local(LocalModelConfig), // Local GGUF model via llama.cpp
}

pub struct CompletionConfig {
    pub max_input_length: usize,       // default: 1536
    pub max_decoding_tokens: usize,    // default: 64
    pub code_search_params: CodeSearchParams,
}
```

### HTTP Model Config (`HttpModelConfig`)

```rust
pub struct HttpModelConfig {
    pub kind: String,              // "llama.cpp/completion", "openai/chat", "llama.cpp/embedding"
    pub api_endpoint: Option<String>,
    pub api_key: Option<String>,
    pub model_name: Option<String>,
    pub prompt_template: Option<String>,
    pub chat_template: Option<String>,
    pub supported_models: Option<Vec<String>>,
    pub rate_limit: RateLimit,     // request_per_minute (default: 1200)
    pub additional_stop_words: Option<Vec<String>>,
}
```

### Local Model Config (`LocalModelConfig`)

```rust
pub struct LocalModelConfig {
    pub model_id: String,
    pub parallelism: u8,               // default: 4 (CPU threads)
    pub num_gpu_layers: u16,           // default: 9999 (all layers to GPU)
    pub enable_fast_attention: Option<bool>,
    pub context_size: usize,           // default: 4096
    pub additional_stop_words: Option<Vec<String>>,
}
```

### Other Key Types

| 类型 | 文件 | 用途 |
|------|------|------|
| `RepositoryConfig` | `config.rs` | Git URL → local dir mapping |
| `ServerConfig` | `config.rs` | completion_timeout (default: 30s) |
| `AnswerConfig` | `config.rs` | RAG search params (min scores, top-K) |
| `EmbeddingConfig` | `config.rs` | embedding max_input_length (default: 5120) |
| `CodeRepository` | `config.rs` | git_url + source_id + git_refs |

### API Types (`api/`)

```
crates/tabby-common/src/api/
├── code.rs        # CodeSearch, CodeSearchParams, CodeSearchDoc types
├── event.rs       # Event logging request/response schemas
├── ingestion.rs   # Custom document ingestion API
├── server_setting.rs # Server setting retrieval schema
├── structured_doc.rs # Structured doc search (page/commit/issue/pull)
└── completion/    # Completion-specific types
```

### Path Utilities (`path.rs`)

```rust
pub fn tabby_root() -> PathBuf       // ~/.tabby or $TABBY_ROOT
pub fn config_file() -> PathBuf      // ~/.tabby/config.toml
pub fn repositories_dir() -> PathBuf // ~/.tabby/repos/
pub fn models_dir() -> PathBuf       // ~/.tabby/models/
pub fn db_path() -> PathBuf          // ~/.tabby/db/default.sqlite [EE]
```

---

## 3.3 crates/tabby-inference — Inference Abstraction

**职责**: 定义所有模型推理的 trait 接口，后端实现解耦。

### Core Traits

```rust
// Completion Stream trait
pub trait CompletionStream: Send + Sync {
    async fn completion(&self, params: CompletionParams) -> Result<CompletionResponse>;
}

// Chat Completion Stream trait
pub trait ChatCompletionStream: Send + Sync {
    async fn chat(&self, messages: Vec<Message>) -> Result<ChatResponse>;
}

// Embedding trait
pub trait Embedding: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}
```

### Internal Modules

| 模块 | 文件 | 功能 |
|------|------|------|
| Decoding | `decoding.rs` | Temperature/top-p/top-k sampling策略 |
| Code | `code.rs` | Completion-specific decoding |
| Chat | `chat.rs` | Chat streaming output processing |
| Completion | `completion.rs` | Generic completion request builder |
| Embedding | `embedding.rs` | Embedding request/response handling |

---

## 3.4 crates/llama-cpp-server — llama.cpp Backend

**职责**: 构建和管理 llama.cpp inference server 进程，处理 GPU 加载和推理请求。

### 核心功能

1. **进程管理** — fork llama.cpp server 作为子进程，通过 HTTP 通信
2. **GPU Offload** — 配置 `--n-gpu-layers`、`-t` threads、CUDA/ROCm/Vulkan backend
3. **GGUF Loading** — 解析 `ggml/model-{i}-of-{n}.gguf` 分片文件

### GPU Backend Selection

```bash
# CMake flags: -DGGML_CUDA=ON | -DGGML_HIP=ON | -DVULKAN=1
--device cuda   # NVIDIA CUDA (默认 GPU backend)
--device rocm   # AMD ROCm
--device vulkan # Vulkan (跨平台 GPU)
```

---

## 3.5 crates/http-api-bindings — HTTP API Adapters

**职责**: 为外部 HTTP API 模型（OpenAI-compatible / Mistral / llamafile 等）提供统一适配层。

### Completion Adapters (`completion/`)

| Adapter | Kind String | 说明 |
|---------|------------|------|
| llama.cpp | `llama.cpp/completion` | llama.cpp `/completion` endpoint |
| OpenAI | `openai/chat` | OpenAI `/v1/chat/completions` |
| Mistral | `mistral/chat` | Mistral `/api/chat` endpoint |
| Any compatible | 自定义 kind | 支持任意 OpenAI-compatible API |

### Embedding Adapters (`embedding/`)

| Adapter | Kind String | 说明 |
|---------|------------|------|
| llama.cpp | `llama.cpp/embedding` | llama.cpp `/embedding` endpoint |
| OpenAI | `openai/embedding` | OpenAI `/v1/embeddings` |
| Mistral | `mistral/embedding` | Mistral embedding API |
| Azure | `azure/embedding` | Azure OpenAI embeddings |

### Chat Adapters (`chat/`)

- 统一将各种后端转换为 `ChatCompletionStream` trait 实现
- 自动处理 SSE streaming / JSON 响应格式差异

---

## 3.6 crates/ollama-api-bindings — Ollama Adapter

**职责**: Ollama-specific 集成，包含模型发现和本地推理。

### 模块

| 文件 | 功能 |
|------|------|
| `model.rs` | `ollama list / show` — 发现和展示本地模型 |
| `completion.rs` | `POST /api/chat` — Ollama chat completions |
| `embedding.rs` | `POST /api/embeddings` — Ollama embeddings |

### API Endpoint Mapping

```
Ollama HTTP API          → Rust trait impl
/api/chat               → ChatCompletionStream
/api/embeddings         → Embedding
/api/list               → model discovery
/api/show {name}        → model metadata (prompt/chat template)
```

---

## 3.7 crates/tabby-index — Indexing Engine

**职责**: 基于 Tantivy 的代码索引 + 结构化文档索引引擎。

### Core Modules (`src/`)

```
tabby-index/src/
├── indexer.rs           # Main indexing orchestration
├── code/               # Code indexing pipeline
│   ├── index.rs        # Tantivy index writer (field definitions, mapping)
│   ├── intelligence.rs # AST-level semantic analysis via tree-sitter
│   ├── repository.rs   # Git repo → source files discovery
│   └── languages.rs    # Language definitions (14+ grammars)
├── structured_doc/     # Structured document indexing
│   ├── types/          # Index schema per entity type
│   │   ├── commit.rs   # Commit message + diff indexing
│   │   ├── ingested.rs # Ingestion metadata tracking
│   │   ├── issue.rs    # GitHub/GitLab Issue indexing
│   │   ├── page.rs     # Web Page indexing
│   │   ├── pull.rs     # Pull Request indexing
│   │   └── web.rs      # Web content indexing
```

### Code Indexing Pipeline

```
Git Repository → tree-sitter AST parsing → Tantivy inverted index
                   │
                   ├── Python (.py)
                   ├── Rust (.rs)
                   ├── TypeScript/JS (.ts/.js)
                   ├── Go (.go)
                   ├── Java (.java)
                   ├── C/C++ (.c/.h/.cpp/.hpp)
                   ├── C# (.cs)
                   ├── Ruby (.rb)
                   ├── Scala (.scala)
                   ├── Kotlin (.kt)
                   ├── Solidity (.sol)
                   ├── Lua (.lua)
                   ├── GDScript (.gd)
                   └── Elixir (.ex/.exs)
```

### Intelligence Analysis (`intelligence.rs`)

tree-sitter 生成的 AST 用于：
- **Symbol Detection**: 函数、类、结构体等符号命名空间
- **Import/Dependency Graph**: import/export 关系追踪
- **Scope Analysis**: 代码块作用域分析

---

## 3.8 crates/tabby-git — Git Repository Operations

**职责**: 对 Git 仓库执行 clone、grep、file_search、commit listing 等操作。

### 核心功能

| 文件 | 功能 | 底层依赖 |
|------|------|---------|
| `serve_git.rs` | HTTP serving of git repos | axum + git2 |
| `grep/mod.rs` | 代码 grep 搜索 (fuzzy) | grep-rs + nucleo |
| `file_search.rs` | 文件路径 fuzzy search | nucleo (fuzzy matcher) |
| `commit.rs` | Commit listing/querying | git2 |

### 用途场景

- **Code Completion Context**: 从本地 repo 拉取相关代码片段
- **Index Ingestion**: 定期 clone/pull 新 commit 到索引
- **Code Browser** [EE]: web UI 中的代码浏览

---

## 3.9 crates/tabby-crawler — Web Content Crawler

**职责**: 爬取自定义 URL、提取页面内容、解析 `llms.txt` 文件。

### 核心组件

| 模块 | 功能 |
|------|------|
| `lib.rs` | Main crawler logic: URL queue → fetch → parse |
| `llms_txt_parser.rs` | Parse `llms.txt` manifest (similar to sitemap.xml) |
| Content extraction | Uses `readable-readability` algorithm (Portions.js in Rust) |

### Crawl Workflow

```
User submits URL(s) in Admin Panel
    → Crawler adds to ingestion queue
    → Cron job picks up URL
    → HTTP GET + readability extraction
    → Text → Tantivy index
    → Mark as ingested (tabby-db)
```

---

## 3.10 crates/tabby-download & aim-downloader

### tabby-download

**职责**: Model download utility with checksum verification and progress tracking.

```bash
# CLI usage
tabby download --model StarCoder-1B
# Downloads to ~/.tabby/models/tabbyml_starcoder-1b/
```

### aim-downloader

**职责**: 内部二进制下载器，提供进度条、断点续传、SHA256 校验。

---

## 3.11 Utility Crates

### hash-ids

**职责**: 将整数 ID 编码为 URL-safe 的混淆字符串（防止枚举攻击）。

```rust
// Salt: "tabby-config-id-serializer"
// Min length: 6
HashIds::new()
    .encode(&[0u64])   → "config:..." (obfuscated)
    .decode("config:...") → vec![0usize]
```

### sqlx-migrate-validate

**职责**: 在编译时验证 `ee/tabby-db/migrations/` 中的迁移文件与 Rust 代码一致性。
