# 第二章：架构总览

## 2.1 High-Level Architecture

Tabby 采用 **三层架构**，严格分离 IDE 端 Agent、API Server 和 Model Backend：

```
┌─────────────┐         ┌─────────────────┐         ┌──────────────────┐
│  Layer 1:   │         │  Layer 2:       │         │  Layer 3:        │
│  IDE Agent  │────────▶│  API Server     │────────▶│  Model Backend   │
│  (TS)       │  HTTP   │  (Rust/Axum)    │  RPC    │  (GGUF/Ollama/   │
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
# Cargo.toml workspace 结构
[workspace]
members = [
    "crates/tabby",         # ← 开源核心 (MIT)
    "crates/tabby-common",
    "crates/tabby-inference",
    "crates/tabby-index",
    "crates/tabby-download",
    "crates/tabby-git",
    "crates/tabby-crawler",
    ...

    # Enterprise Edition (AGPL-3.0)
    "ee/tabby-webserver",
    "ee/tabby-db",
    "ee/tabby-schema",
    "ee/tabby-email",
    "ee/tabby-db-macros",
]
```

| 特性 | `crates/tabby/` | `ee/` |
|------|:---------------:|:-----:|
| 许可 | MIT | AGPL-3.0 |
| 依赖 | 零 EE 依赖 | 依赖 `tabby-common` + SQLx |
| 运行时控制 | 通过 feature flag `--features ee` | 编译时嵌入 |
| API | REST `/v1/*` | GraphQL (Juniper) + Admin UI SPA |

---

## 2.3 Component Dependency Map

### Crates Workspace 依赖图

```
                     ┌──────────────────┐
                     │ crates/tabby     │ ← Server Binary (CLI)
                     │ (main.rs/serve.rs│
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
   │ tabby-index       │  │ tabby-git         │ ← Index/Search
   │ (Tantivy + tree-  │  │ (git2 + nucleo)   │
   │  sitter)          │  └───────────────────┘
   └───────────────────┘
```

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
   │  ServiceLocator)   │  │  50 migrations)    │
   └───────────────────┘  └───────────────────┘

   ┌───────────────────┐  ┌───────────────────┐
   │ ee/tabby-email    │  │ ee/tabby-db-macros│
   │ (SMTP templates)  │  │ (proc macros)      │
   └───────────────────┘  └───────────────────┘
```

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
│  2. Build prompt segments                          │
│  3. POST /v1/completions                           │
└───────────────────┬─────────────────────────────────┘
                    │ HTTP request (JSON)
                    ▼
┌─────────────────────────────────────────────────────┐
│  crates/tabby (服务端 routes/completions.rs)        │
│                                                     │
│  prompt builder                                     │
│  ├── context retrieval (Tantivy code search)       │
│  ├── LSP declarations from local files              │
│  └── recently modified code snippets                │
│                                                      │
│  model inference                                    │
│  ├── llama.cpp / Ollama / OpenAI compatible API     │
│  └── Streaming response (SSE)                       │
└───────────────────┬─────────────────────────────────┘
                    │ streaming completions chunk
                    ▼
┌─────────────────────────────────────────────────────┐
│  tabby-agent (客户端 postprocess/ 20+ rules)       │
│                                                     │
│  Postprocessors (按顺序执行):                        │
│  ├── trimSpace                  ── 修剪空白         │
│  ├── limitScope                 ── 限制作用域       │
│  ├── normalizeIndentation       ── 标准化缩进       │
│  ├── removeDuplicateSuffixLines          ── 去重    │
│  ├── dropMinimum                ── 丢弃过短结果     │
│  ├── removeRepetitiveBlocks             ── 去除重复块│
│  └── ... (20+ total)                                │
│                                                     │
│  输出: inline completion suggestions                 │
└─────────────────────────────────────────────────────┘
```

### Completion Config（默认值）

| 参数 | 默认值 | 含义 |
|------|--------|------|
| `max_input_length` | 1536 (1024+512) | prompt 最大 token 数 |
| `max_decoding_tokens` | 64 | 单次生成 token 上限 |
| `completion_timeout` | 30s | API 超时秒数 |

---

## 2.5 Request Lifecycle: Chat (Answer Engine)

Chat 请求链路比 completion 多了一个 **RAG 检索**阶段：

```
User Message (IDE Chat Panel)
        │
        ▼
┌─────────────────────────────────┐
│ tabby-agent → POST /v1/chat    │
│ (streaming SSE response)        │
└───────────────┬─────────────────┘
                │
                ▼
┌─────────────────────────────────────────────┐
│ Server: Answer Service                      │
│                                             │
│ 1. Retrieve internal context:               │
│    ├── Code search (Tantivy + tree-sitter)  │
│    ├── Document search (web pages/docs)     │
│    └── Repository context                   │
│                                             │
│ 2. Build prompt with retrieval results       │
│ 3. Forward to chat model                     │
│ 4. Stream response back                      │
└─────────────────────────────────────────────┘
```

**Answer Config（默认值）**：

| 参数 | 默认值 | 含义 |
|------|--------|------|
| `num_to_return` | 10 | 返回 top-K 搜索结果 |
| `num_to_score` | 100 | 参与评分的候选数 |
| `min_embedding_score` | 0.5 | embedding 最低相似度 |
| `presence_penalty` | 0.5 | LLM presence penalty |

---

## 2.6 Background Job Scheduler

Tabby 使用 `tokio-cron-scheduler` 实现定时任务：

```
┌─────────────────────────────────────────────┐
│        CronScheduler (tokio)                │
│                                             │
│  Daily Jobs:                                │
│  ├── daily_stats              ── 每日统计    │
│  └── index_garbage_collection ── 索引清理    │
│                                             │
│  Hourly Jobs:                               │
│  ├── index_commits            ── Commit 索引  │
│  ├── index_pages             ── Web Pages 索引 │
│  ├── index_ingestion         ── 文档摄取索引  │
│  └── license_check           ── License 校验  │
│                                             │
│  Periodic Jobs:                             │
│  ├── web_crawler             ── 网页爬虫      │
│  ├── third_party_integration ── GH/GL sync   │
│  └── maintenance (git/db)  ── 日常维护      │
└─────────────────────────────────────────────┘
```

### EE Background Jobs (`ee/tabby-webserver/src/service/job/`)

| Job | 职责 | 频率 |
|-----|------|------|
| `daily.rs` | 生成每日使用统计 | 每日一次 |
| `hourly.rs` | 每小时指标采集 | 每小时 |
| `index_commits.rs` | 新 commit 索引入库 | 每小时 |
| `index_pages.rs` | Web Pages / Custom Documents 索引 | 每小时 |
| `index_ingestion.rs` | 文档摄取队列处理 | 每 15min |
| `license_check.rs` | Enterprise license 有效性检查 | 每小时 |
| `web_crawler.rs` | 自定义 URL 爬取 + 内容提取 | 每日一次 |
| `git.rs` | Git repo 同步与维护 | 每小时 |
| `db.rs` | SQLite vacuum / PRAGMA 维护 | 每日一次 |

---

## 2.7 Data Flow Overview

完整数据流（从初始化到运行时）：

```
┌────────────── Startup Phase ──────────────┐
│                                             │
│ 1. Load config.toml from ~/.tabby/         │
│ 2. Initialize PathConfig (TABBY_ROOT)      │
│ 3. Connect DB (SQLite, WAL mode) [EE]      │
│ 4. Run pending migrations [EE]              │
│ 5. Load models (GGUF / HTTP endpoint)      │
│ 6. Start cron scheduler [EE]               │
│ 7. Build Axum router                       │
│                                             │
└────────────── Runtime Phase ──────────────┘

        Request → Route Handler → Service → Model Backend → Response
           ↑                                                                  ↓
    Context Retrieval ← Tantivy Search ← Index (background job)
```

### 关键路径（文件系统）

| 路径 | 用途 |
|------|------|
| `~/.tabby/config.toml` | 全局配置 |
| `~/.tabby/models/` | GGUF 模型存储 |
| `~/.tabby/repos/` | Git 仓库克隆 |
| `~/.tabby/db/` | SQLite 数据库文件 [EE] |
| `$TABBY_ROOT/` | 根数据目录（可覆盖） |
