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

`ls crates/` 实测正好这 14 个目录。根 `Cargo.toml` 的 `[workspace] members` 则是 18 个：这 14 个 + `ee/` 下 4 个 Rust crate（`tabby-webserver` `tabby-db` `tabby-db-macros` `tabby-schema`）；`ee/tabby-ui`、`ee/tabby-email` 不是 cargo crate，本章不涉及。

---

## 3.1 crates/tabby — Server Binary

**职责**: Tabby 的主入口和 API server，包含 CLI、HTTP routes、业务 service。

### 目录结构

```
crates/tabby/src/                    # 实测 25 个 .rs，分布在 6 个目录（含 src 自身）
├── main.rs          # CLI entry point (subcommands: serve / download)
├── serve.rs         # Server startup, Axum router assembly, OpenAPI merge
├── download.rs      # Model download logic (tabby download)
├── otel.rs          # OpenTelemetry tracing subscriber 初始化
├── routes/          # HTTP API route handlers
│   ├── mod.rs          # run_app：CORS / Prometheus layer / 端口监听
│   ├── completions.rs  # POST /v1/completions
│   ├── chat.rs         # POST /v1/chat/completions（另挂 /v1beta/chat/completions）
│   ├── events.rs       # POST /v1/events
│   ├── health.rs       # GET / POST /v1/health
│   ├── metrics.rs      # GET  /metrics
│   ├── models.rs       # GET  /v1beta/models
│   └── server_setting.rs # GET /v1beta/server_setting
└── services/        # Business logic
    ├── mod.rs             # 声明下面 8 个子模块
    ├── completion.rs      # Code completion service + Segments 定义
    │   └── completion/    # 同名目录放该模块的两个子文件
    │       ├── completion_prompt.rs # prompt builder（768 字符预算拼 snippet）
    │       └── next_edit_prompt.rs  # next-edit / inline edit prompt
    ├── embedding.rs     # Embedding model integration
    ├── tantivy.rs       # IndexReaderProvider：只读打开 ~/.tabby/index
    ├── structured_doc/  # Structured document search (Serper + Tantivy)
    │   ├── mod.rs
    │   ├── serper.rs    # 外部搜索 API 适配
    │   └── tantivy.rs   # doc/commit/issue/pull 检索
    ├── code.rs          # Code search service
    ├── event.rs         # Event logging service（按天写 events/*.json）
    ├── health.rs        # HealthState（设备 / 模型就绪状态）
    └── model/mod.rs     # Model download & loading
```

`services/completion.rs` 与 `services/completion/` 是 Rust 2018 的「模块文件 + 同名目录」组合，不是两个模块；客户端那 13 条 postprocess 规则在 `clients/tabby-agent/src/codeCompletion/postprocess/`，服务端**没有**对应实现。

### Key Subcommands

```bash
# CLI entry point: main.rs parses subcommands
tabby serve      # Start the API server（子命令必填，没有默认子命令）
tabby download   # Download model weights to ~/.tabby/models/
```

`Commands` 枚举只有这两个变体（`crates/tabby/src/main.rs` · `enum Commands+0`）；`Cli` 另有一个隐藏的全局参数 `--otlp-endpoint`，交给 `crates/tabby/src/otel.rs` 初始化 tracing。

### Feature Flags

| Feature | 效果 |
|---------|------|
| `cuda` | 启用 NVIDIA CUDA GPU backend（转发为 `llama-cpp-server/cuda`） |
| `rocm` | 启用 AMD ROCm GPU backend |
| `vulkan` | 启用 Vulkan GPU backend |
| `ee` | 合并 EE webserver routes + types（`dep:tabby-webserver`） |
| `prod` | `ee` + `tabby-webserver/prod`：影响 `db_file()`/`background_jobs_dir()` 取名与启动 spinner |
| `static-ssl` | 把 OpenSSL 以 vendored 方式一起编译 |

`default = ["ee", "llama-cpp-server/binary"]`，即默认构建就带 EE、并且会构建 llama.cpp 二进制；Metal backend 不在 feature 列表里，由 `crates/llama-cpp-server/build.rs` 按目标平台（macOS）自动打开 `LLAMA_METAL=ON`。

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
    pub completion: Option<ModelConfig>,   // 为 None 时 /v1/completions 返回 501
    pub chat: Option<ModelConfig>,          // 驱动 /v1/chat/completions 与 EE Answer Engine
    pub embedding: ModelConfig,             // 非 Option，默认 Local(Nomic-Embed-Text)
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

七个字段的默认值都靠 `#[serde(default)]`，`Config::load` 读不到文件时直接返回 `Default::default()`；解析报错时打印告警并回落到默认配置（`crates/tabby-common/src/config.rs` · `Config::load+0`）。

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
    pub parallelism: u8,               // default: 4 → llama-server 的 -np（并行请求数）
    pub num_gpu_layers: u16,           // default: 9999 (all layers to GPU)
    pub enable_fast_attention: Option<bool>,  // true 时传 -fa
    pub context_size: usize,           // default: 4096 → --ctx-size
    pub additional_stop_words: Option<Vec<String>>,
}
```

三个 default 由 `crates/tabby-common/src/config.rs` · `default_parallelism+0` / `default_num_gpu_layers+0` / `default_context_size+0` 提供；`--device cpu` 时 `crates/tabby/src/main.rs` · `to_local_config+0` 会把 `num_gpu_layers` 直接压成 0，非 CPU 时才用 9999（可被 `LLAMA_CPP_N_GPU_LAYERS` 覆盖）。

### Other Key Types

| 类型 | 文件 | 用途 |
|------|------|------|
| `RepositoryConfig` | `config.rs` | Git URL → local dir mapping（`resolve_dir+0`：`file://` 用本地路径，否则落在 `repositories_dir()` 下） |
| `ServerConfig` | `config.rs` | completion_timeout (default: 30s) |
| `AnswerConfig` | `config.rs` | 只有 3 个字段：`code_search_params` / `presence_penalty` / `system_prompt` |
| `EmbeddingConfig` | `config.rs` | embedding max_input_length (default: 5120) |
| `RateLimit` | `config.rs` | `request_per_minute`（default: 1200），包在 http/ollama 适配层外层 |
| `CodeRepository` | `config.rs` | git_url + source_id + git_refs（索引侧用的运行时结构，不是配置文件里的 `[[repositories]]`） |

### API Types (`api/`)

```
crates/tabby-common/src/api/
├── mod.rs           # 模块声明
├── code.rs          # CodeSearch, CodeSearchParams, CodeSearchDoc types
├── event.rs         # Event logging schemas（`Segments` 也在这里）
├── ingestion.rs     # Custom document ingestion API
├── server_setting.rs # Server setting retrieval schema
└── structured_doc.rs # Structured doc search (page/commit/issue/pull)
```

`api/` 下只有这 6 个文件，**没有** `completion/` 子目录：补全请求体 `CompletionRequest` / `Segments` 定义在服务端自己的模块里（`crates/tabby/src/services/completion.rs` · `struct Segments+0`），`tabby-common` 侧只保留事件日志用的 `Segments`（`crates/tabby-common/src/api/event.rs` · `struct Segments+0`）。

### Path Utilities (`path.rs`)

```rust
pub fn tabby_root() -> PathBuf       // $TABBY_ROOT 或 ~/.tabby（lazy_static TABBY_ROOT）
pub fn config_file() -> PathBuf      // ~/.tabby/config.toml
pub fn usage_id_file() -> PathBuf    // ~/.tabby/usage_anonymous_id
pub fn repositories_dir() -> PathBuf // ~/.tabby/repositories/
pub fn index_dir() -> PathBuf        // ~/.tabby/index/  (Tantivy 索引)
pub fn models_dir() -> PathBuf       // ~/.tabby/models/，可被 TABBY_MODEL_CACHE_ROOT 覆盖
pub fn events_dir() -> PathBuf       // ~/.tabby/events/
```

七个函数就是 `crates/tabby-common/src/path.rs` 的全部内容，**没有** `db_path()`：SQLite 文件路径由 EE 侧决定，见 `ee/tabby-webserver/src/path.rs` · `db_file+0` → `~/.tabby/ee/db.sqlite`（prod）/ `dev-db.sqlite`（dev）。

---

## 3.3 crates/tabby-inference — Inference Abstraction

**职责**: 定义所有模型推理的 trait 接口，后端实现解耦。

### Core Traits

```rust
// crates/tabby-inference/src/completion.rs
#[async_trait]
pub trait CompletionStream: Sync + Send {
    // 流式生成，逐块 yield String
    async fn generate(&self, prompt: &str, options: CompletionOptions) -> BoxStream<'_, String>;
    // 默认实现：把 generate 的 chunk 拼成整串
    async fn generate_sync(&self, prompt: &str, options: CompletionOptions) -> String { .. }
}

// crates/tabby-inference/src/chat.rs
#[async_trait]
pub trait ChatCompletionStream: Sync + Send {
    async fn chat(&self, request: CreateChatCompletionRequest)
        -> Result<CreateChatCompletionResponse, OpenAIError>;
    async fn chat_stream(&self, request: CreateChatCompletionRequest)
        -> Result<ChatCompletionResponseStream, OpenAIError>;
}

// crates/tabby-inference/src/embedding.rs
#[async_trait]
pub trait Embedding: Sync + Send {
    async fn embed(&self, prompt: &str) -> anyhow::Result<Vec<f32>>;
}
```

`CompletionOptions` 字段是 `max_decoding_tokens` / `sampling_temperature` / `seed` / `presence_penalty`（`derive_builder`），两个 chat 方法直接复用 `async_openai_alt::types` 里的 OpenAI 请求/响应类型，所以本 crate 并没有自定义 Message 结构。

### Internal Modules

| 模块 | 文件 | 功能 |
|------|------|------|
| Decoding | `decoding.rs` | `StopConditionFactory` / `StopCondition`：按语言 + 模型配置组装停止词并建 Aho–Corasick trie（没有采样策略） |
| Code | `code.rs` | `CodeGeneration` / `CodeGenerationOptions`：在 `CompletionStream` 外面套 clip_prompt + 停止条件解码 |
| Chat | `chat.rs` | `ChatCompletionStream` trait + `ExtendedOpenAIConfig`（按 kind 改请求 / 拼 headers） |
| Completion | `completion.rs` | `CompletionStream` trait + `CompletionOptions(Builder)` |
| Embedding | `embedding.rs` | 只有一个 `Embedding` trait（`embed`） |
| Root | `lib.rs` | 再导出上面这些类型，并实现 `clip_prompt`（按字符从头部裁剪，保证 UTF-8 边界） |

---

## 3.4 crates/llama-cpp-server — llama.cpp Backend

**职责**: 构建和管理 llama.cpp inference server 进程，处理 GPU 加载和推理请求。

### 核心功能

1. **进程管理** — `supervisor.rs` · `LlamaCppSupervisor::start` 在 `current_exe` 同目录（或 `PATH`）里找 `llama-server` 二进制并 spawn 子进程，随后按 `http://127.0.0.1:{get_available_port()}` 走 HTTP；进程退出/超时由同一个循环负责重启与 fatal 判定。
2. **子进程参数** — `-m {model_path}`、`--cont-batching`、`--port`、`-np {parallelism}`、`--ctx-size`，条件参数：`num_gpu_layers > 0` 时 `-ngl`、`embedding` 时 `--embedding --ubatch-size`（默认 4096，可用 `LLAMA_CPP_EMBEDDING_N_UBATCH_SIZE` 覆盖）、有 chat_template 时 `--chat-template`、`enable_fast_attention` 时 `-fa`、`LLAMA_CPP_N_THREADS` 存在时 `-t`。注意这里**没有** `--n-gpu-layers`，实际写法是 `-ngl`；线程数 `-t` 只能通过环境变量给。
3. **GGUF Loading** — `resolve_model_path+0` 先按本地目录（`{model_id}/ggml`）再按 registry 解析，入口文件靠 `GGML_MODEL_PARTITIONED_PREFIX`（`crates/tabby-common/src/registry.rs` · `static GGML_MODEL_PARTITIONED_PREFIX+0`，值 `"model-00001-of-"`）匹配 `ggml/model-00001-of-{n}.gguf` 分片。
4. **复用 HTTP 适配层** — `EmbeddingServer` / `CompletionServer` / `ChatCompletionServer` 并不自己写请求，而是就地拼一个 `HttpModelConfig`（kind 分别是 `llama.cpp/embedding`、`llama.cpp/completion`、`openai/chat`，endpoint 指向本机端口）交给 `http_api_bindings::create_*`，再套 `rate_limit`。

### GPU Backend Selection

GPU 支持是**编译期**决定的，`crates/llama-cpp-server/build.rs` 里的 CMake define 分别是：`-DGGML_CUDA=ON`（feature `cuda`）、`-DGGML_HIPBLAS=ON` + `AMDGPU_TARGETS`（feature `rocm`，同时把编译器指向 `$ROCM_ROOT/llvm/bin/clang++`）、`-DGGML_VULKAN=ON`（feature `vulkan`）、`-DLLAMA_METAL=ON -DLLAMA_METAL_EMBED_LIBRARY=ON`（`cfg!(target_os = "macos")` 自动开）。另外固定 `LLAMA_CURL=OFF`、`GGML_NATIVE=OFF`、`BUILD_SHARED_LIBS=OFF`。

```bash
# tabby serve 的运行时参数（crates/tabby/src/main.rs · enum Device，默认 cpu）
--device cpu     # 默认值，to_local_config() 会把 num_gpu_layers 压成 0
--device cuda    # NVIDIA CUDA
--device rocm    # AMD ROCm
--device metal   # Apple Metal（macOS 编译期自动开）
--device vulkan  # Vulkan（跨平台 GPU）
# --chat-device 只能配合 --chat_model 使用（clap requires）
```

也就是说 `--device` 只是决定 `-ngl` 层数与健康检查里上报的设备，并不会在运行时切换 CMake 编译出来的 backend。

---

## 3.5 crates/http-api-bindings — HTTP API Adapters

**职责**: 为外部 HTTP API 模型（llama.cpp server / OpenAI 兼容 / Mistral / Azure / Voyage / vLLM / Ollama 等）提供统一适配层。crate 只有 `lib.rs` + `rate_limit.rs` + 三个子目录 `completion/` `chat/` `embedding/`，每条链路的 `create()` 都会把底层 engine 再包一层 `rate_limit::new_*`（限速值来自 `HttpModelConfig.rate_limit.request_per_minute`）。

三个 `create()` 都是按 `model.kind` 字符串 `match`，落到不认识的 kind 直接 `panic!`（所以 kind 拼错是启动期崩溃，不是运行期报错）。

### Completion Adapters (`completion/mod.rs`)

| Kind | 实现文件 | 实际请求的 endpoint |
|------|---------|--------------------|
| `llama.cpp/completion` | `completion/llama.rs` | `{api_endpoint}/completion` |
| `ollama/completion` | 转发 `ollama_api_bindings::create_completion` | 见 3.6 |
| `mistral/completion` | `completion/mistral.rs` | `{base}/v1/fim/completions`（默认 base `https://api.mistral.ai`，走 Codestral FIM） |
| `openai/legacy_completion`、`openai/completion`、`deepseek/completion` | `completion/openai.rs` | `{api_endpoint}/completions`，`use_fim = true` |
| `openai/legacy_completion_no_fim`、`vllm/completion` | `completion/openai.rs` | 同上，`use_fim = false` |

即「任意 OpenAI-compatible 补全 API」是通过上面这 5 个别名 kind 表达的，不是「自定义 kind」；别名清单在 `crates/http-api-bindings/src/completion/mod.rs` · `OPENAI_LEGACY_COMPLETION_FIM_ALIASES+0`。另外 `build_completion_prompt+0` 会对 `mistral/completion` 与三个 FIM 别名强制使用 `{prefix}<|FIM|>{suffix}` 模板。

### Embedding Adapters (`embedding/mod.rs`)

| Kind | 实现文件 | 实际请求的 endpoint |
|------|---------|--------------------|
| `llama.cpp/embedding` | `embedding/llama.rs` | `{api_endpoint}/embeddings` |
| `llama.cpp/before_b4356_embedding` | `embedding/llama.rs` | `{api_endpoint}/embedding`（旧版 llama.cpp 路径） |
| `openai/embedding` | `embedding/openai.rs` | `{api_endpoint}/embeddings` |
| `ollama/embedding` | 转发 `ollama_api_bindings::create_embedding` | 见 3.6 |
| `voyage/embedding` | `embedding/openai.rs` | 默认 base `https://api.voyageai.com/v1`，`model_name` 必填 |
| `azure/embedding` | `embedding/azure.rs` | `{api_endpoint}/openai/deployments/{model}/embeddings`，api-version 常量 `AZURE_API_VERSION = "2024-02-01"` |
| `mistral/embedding` | `embedding/mistral.rs` | `{api_endpoint 或 https://api.mistral.ai/v1}/embeddings` |

`embedding/llama.rs` 还有两个细节：老版本 llama.cpp 的兼容分支靠 `before_b4356` 布尔切换路径，且对 `Connection reset by peer` 一类偶发失败做最多 3 次重试。

### Chat Adapters (`chat/mod.rs`)

- 只有三个 kind：`azure/chat`、`openai/chat`、`mistral/chat`，前两者共用 `OpenAIConfig`（`mistral/chat` 也是），`azure/chat` 用 `AzureConfig` 且 `model_name` 当作 deployment id
- 统一把后端包装成 `ChatCompletionStream`（`chat` / `chat_stream`），SSE 与 JSON 差异交给 `async_openai_alt`
- `kind` 与 `supported_models` 会塞进 `ExtendedOpenAIConfig`（`crates/tabby-inference/src/chat.rs` · `struct ExtendedOpenAIConfig+0`）：`process_request+0` 按 kind 改写请求，`supported_models` 则用来拒绝清单外的 model；`/v1beta/models` 的模型清单是另一条路，直接读 `HttpModelConfig.supported_models`（`crates/tabby/src/routes/models.rs` · `impl From<Config>+0`）

---

## 3.6 crates/ollama-api-bindings — Ollama Adapter

**职责**: Ollama-specific 集成，包含模型发现和本地推理。四个文件 `lib.rs` `model.rs` `completion.rs` `embedding.rs`；HTTP 细节不在这个 crate 里手写，而是依赖 `ollama-rs`（根 workspace 里锁到 git rev `56e8157`，features = `["stream"]`，用 git 版本是为了支持自定义 URL）。

### 模块

| 文件 | 功能 |
|------|------|
| `model.rs` | `trait OllamaModelExt`：`model_available`（`show_model_info`）、`get_first_available_model`（`list_local_models`）、`select_model_or_default`、`pull_model_with_tracing`（`pull_model_stream`） |
| `completion.rs` | `impl CompletionStream for OllamaCompletion`，用 `generate_stream`（Ollama `/api/generate`），模板固定 `{{ .Prompt }}` |
| `embedding.rs` | `impl Embedding`，用 `generate_embeddings`（`/api/embeddings`）。注意这里的结构体名字也叫 `OllamaCompletion`（`crates/ollama-api-bindings/src/embedding.rs` · `pub struct OllamaCompletion+0`），是复制粘贴遗留，两个模块同名但互不冲突 |

### API Endpoint Mapping

```
ollama-rs 调用              → 实际 Ollama endpoint   → Rust trait impl
generate_stream            → /api/generate          → CompletionStream
generate_embeddings        → /api/embeddings        → Embedding
list_local_models          → /api/tags              → 模型发现（model.rs）
show_model_info            → /api/show             → 模型是否存在（model.rs）
pull_model_stream          → /api/pull              → 拉取模型（model.rs）
```

三点需要注意：①`completion.rs` 里有个 FIXME——`options.presence_penalty` 目前没传给 Ollama；②本 crate **没有** chat 适配（kind 清单里也查不到 `ollama/chat`），要用 Ollama 的对话模型得走它的 OpenAI 兼容端点 + `openai/chat`；③只有设了 `TABBY_OLLAMA_ALLOW_PULL`（`1`/`y`/`yes`）才允许自动 pull 缺失模型，否则直接报错；`model_name` 没配且没配模板时会取列表里的第一个模型。

---

## 3.7 crates/tabby-index — Indexing Engine

**职责**: 基于 Tantivy 的代码索引 + 结构化文档索引引擎。

### Core Modules (`src/`)

```
crates/tabby-index/src/
├── lib.rs                # 只对外暴露 `public` 模块；内部再导出 Indexer / IndexAttributeBuilder
├── indexer.rs            # 通用索引编排：Indexer / TantivyDocBuilder / IndexGarbageCollector
├── tantivy_utils.rs      # 索引目录、schema、writer 的公共底座
├── indexer_tests.rs / testutils.rs   # #[cfg(test)] 才编译
├── code/                 # 代码索引链路
│   ├── mod.rs            # CodeIndexer：把 SourceCode 写入 code corpus
│   ├── index.rs          # Tantivy index writer（字段定义、mapping）
│   ├── intelligence.rs   # tree-sitter tags + 分块（内含 metrics 子模块）
│   │   └── intelligence/id.rs   # SourceFileId：内容寻址的文件 key
│   ├── languages.rs      # 15 个语言的 TagsConfiguration（unsafe impl Send/Sync 就在这里）
│   ├── repository.rs     # sync_repository / resolve_commits / checkout / garbage_collection（git2）
│   ├── types.rs          # SourceCode / Tag / Point
│   └── snapshots/        # insta 快照（code_splitter 4 个 .snap）
└── structured_doc/       # 结构化文档索引
    ├── mod.rs
    ├── public.rs         # StructuredDoc{Commit,Issue,Pull,Page,Web,Ingested} 对外类型
    ├── types.rs
    └── types/            # 每种实体一套 schema
        ├── commit.rs     # Commit message 索引
        ├── ingested.rs   # Ingestion 元数据跟踪
        ├── issue.rs      # GitHub/GitLab Issue 索引
        ├── page.rs       # Web Page 索引
        ├── pull.rs       # Pull Request 索引
        └── web.rs        # Web content 索引
```

### Code Indexing Pipeline

```
Git Repository → tree-sitter tags + 分块 → Tantivy inverted index (+ embedding vector)
                   │
                   ├── python (.py)                    ├── c (.c/.h)
                   ├── rust (.rs)                      ├── cpp (.cpp/.hpp/.cc/.C…)
                   ├── javascript-typescript           ├── csharp (.cs)
                   │        (.js/.ts/.jsx/.tsx/.mjs/.mts)├── ruby (.rb)
                   ├── go (.go)                        ├── scala (.scala/.sbt)
                   ├── java (.java)                    ├── kotlin (.kt/.kts)
                   ├── solidity (.sol)                 ├── lua (.lua)
                   ├── gdscript (.gd/.tscn/.tres)      └── elixir (.ex/.exs)
```

`crates/tabby-index/src/code/languages.rs` · `LANGUAGE_TAGS+0` 里正好 15 个 key（上面 15 行），扩展名到语言的映射来自 `crates/tabby-common/assets/languages.toml`（`[[config]]` 共 33 条，即 33 种可识别语言/文件类型，但只有这 15 种配了 tree-sitter tags，其余走纯文本切分）。

### Intelligence Analysis (`code/intelligence.rs`)

`CodeIntelligence` 实际做四件事，`find_tags+0` 的 tag 结构里没有 import/scope 概念：
- **符号标注**：`generate_tags` 产出 `Tag { range, name_range, utf16_column_range, line_range, docs, is_definition, syntax_type_name, span }`，区分定义/引用并带 docstring
- **分块**：`chunks+0` 对配了 tags 的语言用 `CodeSplitter`（`CHUNK_SIZE = 512`），否则退回 `TextSplitter`，同时回算每块起始行号
- **文件身份**：`compute_source_file_id+0` / `check_source_file_id_matched+0` 生成/校验内容寻址 key，索引 GC 靠它判断文件是否已删除或被改
- **代码度量**：内部 `metrics::compute_metrics+0` 给出 `max_line_length` / `avg_line_length` / `alphanum_fraction` / `num_lines` / `number_fraction`，随文档一起入库

原文写的「Import/Dependency Graph 追踪」「Scope Analysis」在代码里没有对应实现，tree-sitter 在这里只用来取 symbol tag 和按语法边界切块。

---

## 3.8 crates/tabby-git — Git Repository Operations

**职责**: 对 Git 仓库执行 clone、grep、file_search、commit listing 等操作。

### 核心功能

| 文件 | 功能 | 底层依赖 |
|------|------|---------|
| `serve_git.rs` | HTTP serving of git repos（供 EE 代码浏览 / git 智能服务） | axum + git2 |
| `grep/mod.rs` | 代码 grep 搜索（`GrepSearcher`） | `grep = "0.3.1"`（regex matcher + searcher），**不是** nucleo |
| `file_search.rs` | 文件路径 fuzzy search | `nucleo`（`nucleo::Matcher` + `AtomKind::Fuzzy`） |
| `commit.rs` | Commit listing / `stream_commits` 流式遍历 | git2 |

`crates/tabby-git/src/` 实际只有 8 个文件：上表 4 个 + `lib.rs` + `grep/{output.rs, query.rs, searcher.rs}`。nucleo 只出现在 `file_search.rs`，grep 链路是正则；把两者混为一谈是常见误解。

### 用途场景

- **Index Ingestion**: 定期 clone/pull 新 commit、按 ref checkout 后重建索引（调用方是 `crates/tabby-index`，见 `code/repository.rs`）
- **Code Browser** [EE]: web UI 中的代码浏览、`serve_git.rs` 的 git HTTP 服务
- **Code Completion Context**: 补全请求链路里并不直接调用 tabby-git——`crates/tabby` 连 `tabby-git` 都没有依赖，它只查已经建好的 Tantivy 索引

---

## 3.9 crates/tabby-crawler — Web Content Crawler

**职责**: 爬取自定义 URL、提取页面内容、解析 `llms.txt` 文件。

### 核心组件

| 模块 | 功能 |
|------|------|
| `lib.rs` | `crawl_pipeline+0`：spawn 外部 `katana` 二进制，逐行读它的 JSONL stdout；`to_document+0` 把一条 HTML 响应转成 Markdown；`crawler_llms+0` 直接 GET `llms-full.txt` |
| `llms_txt_parser.rs` | `split_llms_content+0`：按一级标题（`# `）把 `llms-full.txt` 切成多段，识别段内的 `URL:` / `Source:` 行——**不是** sitemap 式的链接解析 |
| `types.rs` | `KatanaRequestResponse` / `KatanaRequest` / `KatanaResponse`（katana JSONL 行的反序列化结构）、`CrawledMetadata`、`CrawledDocument` |
| Content extraction | 正文抽取交给 `readable-readability = "0.4.0"`（`crates/tabby-crawler/Cargo.toml`，在 `lib.rs` 里以 `use readable_readability::Readability` 引入），HTML→Markdown 交给 `htmd = "0.1"`，crate 本身不做启发式打分 |

`crates/tabby-crawler/src/` 实测只有 `lib.rs llms_txt_parser.rs types.rs` 三个文件。抓取**不是**用 reqwest 自己遍历 URL：`lib.rs · crawl_url+0` 组装的是 `katana -u <start> -jsonl -crawl-scope <prefix>.* -crawl-out-scope "\.js$|\.css$|\.png$|\.jpg$|\.jpeg$" -depth 9999 -max-response-size 10485760 -rate-limit-minute 120 -strategy breadth-first`；设了 `TABBY_CRAWL_ENABLE_HEADLESS` 才会追加 `-headless`。也就是说 katana 是运行时外部依赖，没装就直接 `Failed to run katana` 报错。

### Crawl Workflow

```
Admin Panel 提交 URL
    → ee/tabby-webserver/src/service/web_documents.rs · create_custom_web_document+0
        → job_service.trigger(...)：往 job_runs 插一行（已 pending 则复用）
    → background_job 调度循环取出到期的 BackgroundJobEvent::WebCrawler
    → WebCrawlerJob::run+0（整体 tokio::time::timeout 包住，上限 7200s）
    → 先试 crawler_llms（llms-full.txt 按 H1 切段），失败则 crawl_pipeline
    → Readability 去噪 + htmd 转 Markdown
    → StructuredDocIndexer 写 Tantivy 的 web_documents 索引
```

两点容易被图里那句话带偏：（1）这里没有专门的「Cron job」，`WebCrawlerJob` 不是定时任务，由 GraphQL 建/改文档时 `trigger` 入队，消费分支在 `ee/tabby-webserver/src/service/background_job/mod.rs`（`BackgroundJobEvent::WebCrawler` 的 match arm）；（2）末尾不是「Mark as ingested (tabby-db)」——爬完只写索引，`web_documents` 表里的 URL 记录在创建时就已经落库。另外 embedding 未启用时该 job 会直接跳过，见 `background_job/mod.rs` 里 `Embedding is disabled` 分支。

---

## 3.10 crates/tabby-download & aim-downloader

### tabby-download

**职责**: Model download utility with checksum verification and progress tracking.

```bash
# CLI usage（`DownloadArgs` 只有两个参数：`--model` 与 `--prefer-local-file`，后者默认 false）
tabby download --model StarCoder-1B
tabby download --model TabbyML/StarCoder-1B     # 两种写法等价

# 落盘位置：~/.tabby/models/TabbyML/StarCoder-1B/{tabby.json, ggml/}
```

`--model` 的值由 `crates/tabby-common/src/registry.rs · parse_model_id+0` 解析：按 `/` 切一段时默认 registry 为 `TabbyML`，两段时是 `<registry>/<name>`，**三段以上直接 `panic!("Invalid model id {model_id}")`**。目录形态在 `registry.rs` 的注释与 `get_model_store_dir+0` 里写死为 `{models_dir}/{registry}/{name}/ggml`（例：`~/.tabby/models/TabbyML/StarCoder-1B/ggml`），元数据 `tabby.json` 放在 `{name}` 这一层。

### aim-downloader

**职责**: 内部二进制下载器，提供进度条、断点续传、SHA256 校验。

---

## 3.11 Utility Crates

### hash-ids

**职责**: 将整数 ID 编码为 URL-safe 的混淆字符串（防止枚举攻击）。

```rust
// crates/hash-ids/src/lib.rs 只有 builder 入口，没有 `HashIds::new()`
let hash_ids = HashIds::builder()
    .with_salt("tabby-config-id-serializer")
    .with_min_length(6)
    .finish();

hash_ids.encode(&[0u64])        // -> String（"config:" 前缀由调用方拼）
hash_ids.decode("…")            // -> Option<Vec<u64>>，认不出的串返回 None
```

`crates/hash-ids/src/lib.rs · HashIds::builder+0` 出来的 builder 有三个方法：`with_salt`、`with_min_length`、`with_alphabet`；不带 alphabet 时 `finish()` 直接返回 `HashIds`，带了自定义字母表则返回 `Result<HashIds, Error>`（去重后 separators + alphabet 不足 16 个字符报 `AlphabetTooSmall`，含空格报 `ContainsSpace`）。`encode(&[u64]) -> String`、`decode(&str) -> Option<Vec<u64>>` 见同文件 · `encode+0` / `decode+0`。

仓库里三处实例（全是 `lazy_static!`）：

| Salt | 位置 | 用途 |
|------|------|------|
| `tabby-config-id-serializer`（min_length 6） | `crates/tabby-common/src/config.rs · HASHER+0` | `config_index_to_id+0` 把 config.toml 里 `[[repositories]]` 的下标编成 `config:{hash}`，`config_id_to_index+0` 反向先 `strip_prefix("config:")` 再取 `decode().first()` |
| `tabby-id-serializer`（min_length 6） | `ee/tabby-schema/src/dao.rs · HASHER+0` | `AsID`/`AsRowid`：DB 的 `i64` rowid ↔ GraphQL `juniper::ID` |
| `tabby-refresh-token`（min_length 6） | `ee/tabby-db/src/refresh_tokens.rs · HASHER+0` | `generate_refresh_token+0`：hash 后再拼一段 UUID |

### sqlx-migrate-validate

**职责**: 在**运行时**校验「数据库里已应用的迁移」与「二进制内嵌的迁移」是否一致——不是编译期检查。

`sqlx::migrate!("./migrations")` 这个宏在编译时把 `ee/tabby-db/migrations/` 打进二进制，`validate()` 则必须拿一个连接在运行时执行：`crates/sqlx-migrate-validate/src/validate.rs · Validator::validate+0` 依次查 dirty version、比对每条迁移的 `version` 是否存在于 `_sqlx_migrations`、以及 `checksum` 是否相同，不一致时返回 `ValidateError::VersionNotApplied` / 校验和不匹配的 `MigrateError`；`validate_applied_migrations+0` 反向检查「库里应用过的版本在源码迁移里不存在」的情况。

全仓库只有一个生产调用点：`ee/tabby-db/src/lib.rs · backup_db+0` 用 `sqlx::migrate!("./migrations").validate(&mut *conn)` 判断是否需要迁移——validate 通过说明无需迁移，跳过备份；否则先把 db 文件复制成 `db.backup-${date}.sqlite`（非 prod 为 `dev-db.backup-${date}.sqlite`），再由 `init_db+0` 执行 `sqlx::migrate!("./migrations").run(&pool)`。
