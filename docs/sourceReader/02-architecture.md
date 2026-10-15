# 第二章：架构总览

## 2.1 High-Level Architecture

Tabby 采用 **三层架构**，严格分离 IDE 端 Agent、API Server 和 Model Backend：

```
┌─────────────┐         ┌─────────────────┐         ┌──────────────────┐
│  Layer 1:   │         │  Layer 2:       │         │  Layer 3:        │
│  IDE Agent  │────────▶│  API Server     │────────▶│  Model Backend   │
│  (TS)       │  HTTP   │  (Rust/Axum)    │  HTTP   │  (GGUF/Ollama/   │
│             │<────────│                 │<────────│   OpenAI API     │
│  completion │ Streaming│  REST + GraphQL │  Result  │                  │
│  + chat     │ Streams │  + Jobs + Auth  │         └──────────────────┘
└─────────────┘          └─────────────────┘
```

### 设计原则

| 原则 | 说明 |
|------|------|
| **Self-contained** | 零外部依赖（无需 PostgreSQL/Redis），SQLite 即够 |
| **Feature Flag 隔离** | EE 功能通过 `--features ee` 编译开关控制，不影响开源核心 |
| **Backend Abstraction** | `http-api-bindings` + `ollama-api-bindings` 统一适配层 |
| **Indexing Decoupled** | Tantivy 索引与 server 解耦，可独立维护 |

---

## 2.2 Open Source Core vs Enterprise Edition

### 编译时边界

```toml
# 根 Cargo.toml 的 [workspace] members（共 18 个）
[workspace]
members = [
    "crates/tabby",         # ← 开源核心 (Apache-2.0)
    "crates/tabby-common",
    "crates/tabby-inference",
    "crates/tabby-index",
    "crates/tabby-download",
    "crates/tabby-git",
    "crates/tabby-crawler",

    "crates/aim-downloader",
    "crates/http-api-bindings",
    "crates/llama-cpp-server",
    "crates/ollama-api-bindings",
    "crates/tabby-index-cli",
    "crates/hash-ids",
    "crates/sqlx-migrate-validate",

    # Enterprise Edition（商业授权，见 ee/LICENSE）
    "ee/tabby-webserver",
    "ee/tabby-db",
    "ee/tabby-db-macros",
    "ee/tabby-schema",
]
```

实测 `crates/` 14 个 + `ee/` 4 个 = 18 个 Rust crate。注意 `ee/` 目录下的另外两个子目录**不是 cargo crate**：`ee/tabby-ui` 是 React + Vite 前端 SPA，`ee/tabby-email` 没有 `Cargo.toml`（只有 `README.md` `components/` `emails/` `package.json`，是邮件模板工程），Rust 侧的 SMTP 发送在 `ee/tabby-webserver/src/service/email/`。

| 特性 | `crates/tabby/` | `ee/` |
|------|:---------------:|:-----:|
| 许可 | Apache-2.0（根 `LICENSE`） | Tabby Enterprise License（`ee/LICENSE`，非 OSI 开源） |
| 依赖 | 零 EE 依赖（`ee` feature 关闭时 `tabby-webserver` 不参与编译） | 依赖 `tabby-common` + SQLx |
| 运行时控制 | 通过 feature flag `--features ee` | 编译时嵌入 |
| API | REST `/v1/*` + `/v1beta/*` | GraphQL `/graphql` (Juniper) + Admin UI SPA |

---

## 2.3 Component Dependency Map

### Crates Workspace 依赖图

```
                     ┌──────────────────┐
                     │ crates/tabby     │ ← Server Binary (CLI)
                     │ main.rs·serve.rs │
                     └────┬─────┬───────┘
                          │     │
              requires    │     │   routes/svc layer
            ┌─────────────┘     │
            ▼                   ▼
   ┌──────────────────┐  ┌───────────────────┐
   │ tabby-common     │  │ tabby-inference   │ ← Trait definitions
   │ (Config, types)  │  │ (Completion/Chat  │
   │ (path.rs/api/)   │  │  /Embedding traits)│
   └──────┬───────────┘  └──────┬────────────┘
          │                      │
          │    implements        │
          ▼                      ▼
   ┌───────────────────┐  ┌───────────────────┐
   │ llama-cpp-server  │  │ http-api-bindings  │ ← Backends
   │ (llama.cpp proc)  │  │ + ollama-bindings  │
   └───────────────────┘  └───────────────────┘

   ┌───────────────────┐  ┌───────────────────┐
   │ tabby-index       │  │ tabby-git         │ ← Index/Search（被 ee/tabby-webserver 调用）
   │ (Tantivy + tree-  │  │ (git2 + nucleo)   │
   │  sitter)          │  └───────────────────┘
   └───────────────────┘
```

上面这张图的连线已按 `crates/tabby/Cargo.toml` 逐条复核：`crates/tabby` 直接依赖 `tabby-common`、`tabby-inference`、`http-api-bindings`、`llama-cpp-server`、`tabby-download`（以及 `tantivy`），**并不依赖** `tabby-index` / `tabby-git` / `tabby-crawler`。开源服务端只是**读**索引：`crates/tabby/src/services/tantivy.rs` · `IndexReaderProvider::load` `+0` 用 `Index::open_in_dir(path::index_dir())` 打开已经建好的 Tantivy 索引；写入侧 `tabby-index` + `tabby-git` + `tabby-crawler` 由 EE 的后台任务调用（`ee/tabby-webserver/Cargo.toml`）。

### EE Layer 依赖图（feature = ee）

```
   ┌───────────────────────────────────────────────────────┐
   │            ee/tabby-webserver                         │
   │  (axum router + service layer + background jobs)     │
   └──────────┬────────────────────┬──────────────────────┘
              │                    │
         provides              calls
              ▼                    ▼
   ┌───────────────────┐  ┌───────────────────┐
   │ ee/tabby-schema   │  │ ee/tabby-db       │
   │ (GraphQL schema +  │  │ (SQLite/SQLx +    │
   │  ServiceLocator)   │  │  49 个 up 迁移)    │
   └───────────────────┘  └───────────────────┘

   ┌───────────────────┐  ┌───────────────────┐
   │ ee/tabby-email    │  │ ee/tabby-db-macros│
   │ (邮件模板工程,     │  │ (proc macro:       │
   │  非 cargo crate)  │  │  query_paged_as)  │
   └───────────────────┘  └───────────────────┘
```

`ee/tabby-db/migrations/` 实测为 49 个 `.up.sql` + 49 个 `.down.sql`（编号到 `0050_add-repository-refs`，`0032` 缺号），由 `ee/tabby-db/src/lib.rs` 里的 `sqlx::migrate!("./migrations")` 在启动时先 `validate()` 再 `run()`。`ee/tabby-email/` 没有 `Cargo.toml`，只含 `README.md`、`components/`、`emails/`（4 个 `.tsx` 模板）、`package.json`；Rust 侧的 SMTP 发送与模板渲染在 `ee/tabby-webserver/src/service/email/`（`mod.rs` + `templates.rs`）。

---

## 2.4 Request Lifecycle: Code Completion

这是 Tabby 最核心的请求链路，涉及多层处理：

```
IDE Cursor Position
        │
        ▼
┌─────────────────────────────────────────────────────┐
│  tabby-agent (客户端)                               │
│  1. Capture context: prefix + suffix code          │
│  2. Build prompt segments (LSP declarations、      │
│     changed / recently opened files 的 snippets)    │
│  3. POST /v1/completions                           │
└───────────────────┬─────────────────────────────────┘
                    │ HTTP request (JSON)
                    ▼
┌─────────────────────────────────────────────────────┐
│  crates/tabby (服务端 routes/completions.rs)        │
│                                                     │
│  prompt builder                                     │
│  ├── 预算 768 字符，先吃客户端 segments：          │
│  │   declarations(优先级最高) → changed files       │
│  │   → recently opened files                        │
│  ├── 剩余额度 > 256 且 git_url 命中允许仓库时，     │
│  │   才做 Tantivy code search 补 snippets           │
│  └── build_prefix() 把 snippets 拼进 prompt         │
│                                                      │
│  model inference                                    │
│  ├── llama.cpp / Ollama / OpenAI compatible API     │
│  └── Streaming response (SSE)                       │
└───────────────────┬─────────────────────────────────┘
                    │ streaming completions chunk
                    ▼
┌─────────────────────────────────────────────────────┐
│  tabby-agent (客户端 postprocess/，13 条规则)       │
│                                                     │
│  两段流水线（index.ts 里按顺序串起来）:              │
│  preCacheProcess (5):                               │
│  ├── trimMultiLineInSingleLineMode                  │
│  ├── removeLineEndsWithRepetition                    │
│  ├── dropDuplicated                                │
│  ├── trimSpace                  ── 修剪空白         │
│  └── dropMinimum                ── 丢弃过短结果     │
│                                                     │
│  postCacheProcess (10):                              │
│  ├── removeRepetitiveBlocks ── 去除重复块            │
│  ├── removeRepetitiveLines                          │
│  ├── limitScope                 ── 限制作用域       │
│  ├── removeDuplicatedBlockClosingLine              │
│  ├── formatIndentation / normalizeIndentation      │
│  │                              ── 标准化缩进       │
│  ├── dropDuplicated / trimSpace                    │
│  └── removeDuplicateSuffixLines          ── 去重    │
│                                                     │
│  输出: inline completion suggestions                 │
└─────────────────────────────────────────────────────┘
```

客户端规则清单见 `clients/tabby-agent/src/codeCompletion/postprocess/index.ts` · `preCacheProcess` / `postCacheProcess`，规则实现文件共 13 个（另有 `base.ts`、`index.ts`）。服务侧 snippet 组装逻辑见 `crates/tabby/src/services/completion/completion_prompt.rs` · `PromptBuilder::collect` `+0`（`quota_threshold_for_snippets_from_code_search = 256`、`max_snippets_chars_in_prompt = 768`）与 `extract_snippets_from_segments+0`。

### Completion Config（默认值）

| 参数 | 默认值 | 含义 |
|------|--------|------|
| `max_input_length` | 1536 (1024+512) | prompt 最大长度 |
| `max_decoding_tokens` | 64 | 单次生成 token 上限 |
| `completion_timeout` | 30s | `/v1/completions` 路由的 TimeoutLayer 秒数 |

三个默认值分别取自 `crates/tabby-common/src/config.rs` · `default_max_input_length+0`、`default_max_decoding_tokens+0`、`ServerConfig::default+0`；超时在 `crates/tabby/src/serve.rs` · `api_router+0` 内 `TimeoutLayer::new(Duration::from_secs(config.server.completion_timeout))` 生效。

---

## 2.5 Request Lifecycle: Chat (Answer Engine)

需要先划清边界：**开源侧的 `/v1/chat/completions` 不做检索**，`crates/tabby/src/routes/chat.rs` · `chat_completions+0` 只把 `CreateChatCompletionRequest` 交给 `ChatState { chat_completion, logger }` 透传给 chat 模型并以 SSE 回流。**带 RAG 检索的 Answer Engine 属于 EE**，比上面的链路多了一个检索阶段：

```
User Message (Admin UI / IDE Chat Panel)
        │
        ▼
┌─────────────────────────────────┐
│ GraphQL /graphql (Juniper)      │
│ → thread.create → AnswerService │
└───────────────┬─────────────────┘
                │
                ▼
┌─────────────────────────────────────────────┐
│ EE: ee/tabby-webserver/src/service/answer.rs│
│                                             │
│ 1. Retrieve context (RetrievalService):     │
│    ├── Code search (Tantivy + tree-sitter)  │
│    │   find_repository → collect_file_list  │
│    │   → collect_relevant_code              │
│    ├── Document search (web pages/docs)     │
│    │   collect_relevant_docs                │
│    └── Repository / policy(allowed repos)   │
│                                             │
│ 2. Build prompt with retrieval results       │
│ 3. Forward to chat model                     │
│ 4. Stream response back                      │
└─────────────────────────────────────────────┘
```

**Answer Config（默认值）**：

| 参数 | 默认值 | 含义 |
|------|--------|------|
| `answer.code_search_params.num_to_return` | 10 | 返回 top-K 搜索结果 |
| `answer.code_search_params.num_to_score` | 100 | 参与评分的候选数 |
| `answer.code_search_params.min_embedding_score` | 0.5 | embedding 最低相似度 |
| `answer.presence_penalty` | 0.5 | LLM presence penalty |

前三个不是 `AnswerConfig` 的直属字段，而是它内嵌的 `CodeSearchParams`，默认值来自 `crates/tabby-common/src/config.rs` · `default_answer_code_search_params+0`（同处还给出 `min_bm25_score = -1.0`、`min_rrf_score = -1.0`）；`presence_penalty` 才是 `AnswerConfig` 的字段（`default_presence_penalty+0`）。对照：补全侧的 `completion.code_search_params` 用 `CodeSearchParams::default+0`（`min_embedding_score 0.75` / `min_bm25_score 8.0` / `num_to_return 20` / `num_to_score 40`）。

---

## 2.6 Background Job Scheduler

定时任务**不是** `tokio-cron-scheduler` 驱动的（根 `Cargo.toml` 的 `[workspace.dependencies]` 里有这一条，但全仓没有任何 `Cargo.toml` 引用它、也没有 `tokio_cron_scheduler::` 调用点）。真实实现是 `cron::Schedule` + 自研的 `CronStream`（`ee/tabby-webserver/src/service/background_job/helper/cron.rs` · `CronStream::new+0`，固定 UTC），再由 `background_job/mod.rs` · `start+0` 里一个 `tokio::select!` 循环同时驱动「定时器」和「任务队列」：

```
┌─────────────────────────────────────────────┐
│ background_job::start (tokio::select!)      │
│                                             │
│ 三条 CronStream (cron crate, UTC):          │
│  ├── @hourly        → Hourly               │
│  ├── @daily         → Daily                 │
│  └── */10 * * * * * → ingestion_index_sync  │
│        （每 10 秒查一次是否有待摄取文档）     │
│                                             │
│ 任务队列侧:                                  │
│  └── db.get_next_job_to_execute()           │
│        空则 sleep 5s；job_runs 表落库/记日志 │
│                                             │
│ Hourly 派生出的子任务:                       │
│  ├── db_maintenance / scheduler_git         │
│  ├── third_party_repository_sync            │
│  ├── scheduler_github_gitlab / page_index   │
│  └── index_garbage_collection               │
│                                             │
│ Daily 派生: └── license_check               │
└─────────────────────────────────────────────┘
```

### EE Background Jobs (`ee/tabby-webserver/src/service/background_job/`)

| Job | 职责 | 触发方式 |
|-----|------|---------|
| `hourly.rs` (`NAME = "hourly"`) | 派发 db/git/integration/third-party 子任务，并直接跑一次索引 GC | `@hourly` |
| `daily.rs` (`"daily"`) | 只做一件事：派发 `license_check` | `@daily` |
| `git.rs` (`"scheduler_git"`) | clone/pull 配置的 git repo 并重建索引 | `HourlyJob::run` 内逐仓库入队 |
| `index_commits.rs` | 非独立任务：`refresh()` 写 commit 索引 + GC，由 `git.rs` / `third_party_integration.rs` 调用 | 随上游任务 |
| `index_pages.rs` (`"page_index_sync"`) | Web Pages / Custom Documents 索引 | `@hourly` 分支内 `SyncPageIndexJob::cron` |
| `index_ingestion.rs` (`"ingestion_index_sync"`) | 文档摄取队列处理 | 每 10 秒检查，`should_ingest()` 为真才入队 |
| `license_check.rs` (`"license_check"`) | Enterprise license 过期提醒（Community 直接返回） | 由 `daily.rs` 派发 |
| `web_crawler.rs` (`"web_crawler"`) | 自定义 URL 爬取 + 内容提取 | **不是定时**：保存 Web Documents 时由 `ee/tabby-webserver/src/service/web_documents.rs` `trigger` |
| `third_party_integration.rs` (`"third_party_repository_sync"` / `"scheduler_github_gitlab"`) | GH/GL issue+PR 同步与索引（设了 `TABBY_INDEX_REPO_IN_SHARD` 且仓库数 > 20 时，按每片 7 个仓库轮转分片） | `HourlyJob::run` 派发 |
| `index_garbage_collection.rs` (`"index_garbage_collection"`) | 删除已不存在仓库的索引 | `Hourly` 直接执行；删仓库时也会 trigger |
| `db.rs` (`"db_maintenance"`) | 清理过期 token / 密码重置 / 临时 thread、无用 access_policy、`data_retention` 保留策略 | `HourlyJob::run` 派发 |

注意 `hourly.rs` 只采集任务派发所需的状态并串起子任务，「每日使用统计」并不在这些文件里，它由 `ee/tabby-webserver/src/service/analytic.rs` 按 DB 记录聚合。

---

## 2.7 Data Flow Overview

完整数据流（从初始化到运行时）：

```
┌────────────── Startup Phase ──────────────┐
│                                             │
│ 1. Config::load() 读 ~/.tabby/config.toml  │
│ 2. 建 tabby_root() 并 chmod 0700（Unix）   │
│ 3. merge_args + load_model（GGUF 下载 /    │
│    HTTP endpoint 由 config 决定）           │
│ 4. embedding::create（仅当                 │
│    TABBY_EMBEDDING_ENABLED=yes）            │
│ 5. [EE] Webserver::new：连 SQLite(WAL) +   │
│    必要时备份 + 跑 pending migrations       │
│ 6. IndexReaderProvider / CodeSearch /      │
│    DocSearch / CompletionService / ChatState│
│ 7. api_router + OpenAPI（EE doc 合并）+     │
│    SwaggerUi；[EE] ws.attach() 挂 /graphql │
│ 8. [EE] create_services 内 background_job::│
│    start 拉起定时任务；run_app 监听端口     │
│                                             │
└────────────── Runtime Phase ──────────────┘

        Request → Route Handler → Service → Model Backend → Response
           ↑                                                                  ↓
    Context Retrieval ← Tantivy Search ← Index (background job)
```

步骤 5 的「先备份再迁移」顺序在 `ee/tabby-db/src/lib.rs` · `DbConn::new+0` → `backup_db+0`（用 `sqlx::migrate!("./migrations").validate()` 判断是否需要迁移，需要则复制成 `db.backup-YYYYMMDD.sqlite`）→ `init_db+0`（`run()`）；`background_job::start` 在 `ee/tabby-webserver/src/service/mod.rs` · `impl ServerContext` · `new+0` 内被调用，`ServerContext` 又由 `create_service_locator+0` 构造（调用点 `ee/tabby-webserver/src/webserver.rs` · `Webserver::attach+0`）。

### 关键路径（文件系统）

| 路径 | 用途 |
|------|------|
| `~/.tabby/config.toml` | 全局配置（`crates/tabby-common/src/path.rs` · `config_file+0`） |
| `~/.tabby/models/` | GGUF 模型存储，可被 `TABBY_MODEL_CACHE_ROOT` 覆盖 |
| `~/.tabby/models/{Registry}/{ModelName}/ggml/` | 分片落盘位置，入口 `model-00001-of-*.gguf` |
| `~/.tabby/repositories/` | Git 仓库克隆（`repositories_dir+0`；`file://` 开头则直接用本地路径） |
| `~/.tabby/index/` | Tantivy 索引目录（`index_dir+0`） |
| `~/.tabby/events/` | 开源侧事件落盘，按天一个 `YYYY-MM-DD.json`（每行一个 JSON；`path.rs` · `events_dir+0`） |
| `~/.tabby/usage_anonymous_id` | 匿名统计 ID（`usage_id_file+0`） |
| `~/.tabby/ee/db.sqlite` | SQLite 数据库文件 [EE]（`ee/tabby-webserver/src/path.rs` · `db_file+0`，非 prod 编译为 `dev-db.sqlite`） |
| `~/.tabby/ee/jobs` | 后台任务日志目录 [EE]（`background_jobs_dir+0`，非 prod 为 `dev-jobs`） |
| `$TABBY_ROOT/` | 根数据目录（环境变量覆盖，默认为 `~/.tabby`） |
