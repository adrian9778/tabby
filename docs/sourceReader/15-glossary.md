# 第十五章：术语表

## Glossary

以下按字母顺序列出 Tabby 项目中的关键术语与概念。每条的定义都回源码核对过：表名、字段名、数量、路径以当前提交为准，不确定的写法一律删掉而不是「大概对」。

---

### A

| 术语 | 定义 |
|------|------|
| **Access Policy** | EE 里的「谁能读哪个 source」映射，落在 `source_id_read_access_policies(source_id, user_group_id)` 表（**没有** `_access_policies` 这张表）。语义是 source → 允许读取的用户组，实现在 `ee/tabby-webserver/src/service/access_policy.rs` + `user_group.rs`；已失效 source 的残留策略由 hourly 的 `DbMaintenanceJob` 清理。 |
| **Agent (tabby-agent)** | 运行在 IDE 进程内的 TypeScript SDK（`clients/tabby-agent/`），负责与 server 通信、构建补全请求、跑 postprocess 管道、以及 chat 交互；各 IDE 插件都薄薄地包在它上面。 |
| **Answer Engine** | EE 的 RAG 对话引擎：把代码检索（Tantivy）、文档索引、用户问题拼成上下文交给聊天模型，输出带引用的回答。 |
| **AST (Abstract Syntax Tree)** | 源码的树形表示。`crates/tabby-index/src/code/languages.rs · LANGUAGE_TAGS` 为 15 种语言配置 tree-sitter tags query，用于抽取符号与作用域信息。 |
| **Axum** | Rust HTTP 框架，server 侧路由与请求处理（`crates/tabby/src/routes/`）。 |

### B

| 术语 | 定义 |
|------|------|
| **BM25** | **Best Match 25**（Okapi BM25），Tantivy 使用的文本相关性打分算法 —— 不是「Boolean Matching」。Tabby 把它与 embedding 相似度做双路召回，再用 RRF 融合。 |

### C

| 术语 | 定义 |
|------|------|
| **Completion Pipeline** | IDE → Agent → Server 的完整补全链：context capture → prompt building → model inference → postprocess → inline display。后处理见 **Postprocessor**。 |
| **Config.toml** | **只有 TOML 一种格式**：`Config::load()` 用 `serdeconv::from_toml_file` 反序列化，没有 YAML 形态、也没有 `${ENV}` 展开。server 侧路径固定 `~/.tabby/config.toml`（`crates/tabby-common/src/path.rs · config_file()`），顶层键共 7 个：`repositories` / `server` / `model` / `completion` / `embedding` / `answer` / `additional_languages`。详见第 12 章。 |
| **Context Provider** | tabby-agent 里收集 IDE 上下文的模块，位于 `clients/tabby-agent/src/contextProviders/`：`workspace.ts`、`documentContexts.ts`、`editorOptions.ts`、`editorVisibleRanges.ts`、`declarationSnippets.ts`、`recentlyChangedCodeSearch.ts`、`git/`。 |
| **CronScheduler** | 真实机制是 `cron` 0.12.1 的 `Schedule` + 仓库自研的 `CronStream`（`ee/tabby-webserver/src/service/background_job/helper/cron.rs`），由 `background_job/mod.rs · start` 里一个 `tokio::select!` 同时驱动 `@hourly`、`@daily`、一个 `*/10 * * * * *` 的快速轮询流和任务队列。**不是** `tokio-cron-scheduler` —— 根 `Cargo.toml` 里有这条 workspace 依赖，但全仓没有任何 `Cargo.toml` 引用它、也没有调用点。 |

### D

| 术语 | 定义 |
|------|------|
| **DbMaintenanceJob** | `Job::NAME = "db_maintenance"`（`ee/tabby-webserver/src/service/background_job/db.rs`），由 `HourlyJob::run` 每小时调用一次。做的事：删过期 refresh token、过期 password reset、过期 ephemeral thread、未被引用的 read access policy，并执行 3 个月数据保留（`job_runs`、`user_events`）。**不做 VACUUM，也不回收磁盘空间。** |

### F

| 术语 | 定义 |
|------|------|
| **FIM (Fill-in-the-Middle)** | 给定 prefix + suffix 让模型预测中间内容的推理模式，是代码补全的基础形态。`prompt_template` 用 `{prefix}` / `{suffix}` 两个占位符（`crates/tabby/src/services/completion/completion_prompt.rs · build_prompt` 的 `strfmt!` 渲染）。 |
| **Fusion (RRF)** | Reciprocal Rank Fusion —— 把 BM25 与 embedding 两路排序合并成最终顺序；对应 `CodeSearchParams::min_rrf_score`。 |

### G

| 术语 | 定义 |
|------|------|
| **GGUF** | llama.cpp 使用的模型权重容器格式。Tabby 的本地模型放在 `~/.tabby/models/<org>/<name>/`，权重在 `ggml/` 子目录，另有 `tabby.json` 描述 prompt 模板等元数据；由 `llama-cpp-server` crate 拉起子进程加载。 |
| **GraphQL** | EE 管理面协议。实测字段数：**43 Queries / 62 Mutations / 5 Subscriptions**（`ee/tabby-schema/src/schema/mod.rs` 里两个 `#[graphql_object]` 块 + 一个 `#[graphql_subscription]` 块）。5 个 subscription 是 `createThreadAndRun` / `createThreadRun` / `createPageRun` / `createThreadToPageRun` / `createPageSectionRun`。 |
| **JWT (JSON Web Token)** | 用户访问令牌。**有效期都是写死的**：access token 30 分钟（`ee/tabby-webserver/src/jwt.rs · JWT_DEFAULT_EXP = 30 * 60`），refresh token 7 天（`ee/tabby-db/src/refresh_tokens.rs · create_refresh_token` 里的 SQL 字面量 `datetime('now', '+7 days')`）。登录 mutation 是 `tokenAuth`（LDAP 走 `tokenAuthLdap`）。 |

### I

| 术语 | 定义 |
|------|------|
| **Ingestion** | 用户通过 API 提交的文档片段，落到 `ingested_documents` 表，再由 `SyncIngestionIndexJob` 同步进 Tantivy。与 crawler 抓回来的 web 文档是两条不同的链路。 |
| **Integration (GitHub/GitLab)** | EE 的第三方代码平台集成（`oauth_credential` 表 + `integrations` 表）：同步 Issue/PR/MR/Commit 作为可检索文档，并为用户组提供代码来源策略。 |
| **Index** | Tantivy 索引。Tabby 在 `crates/tabby-index/src/` 下分两个写入面：`code/`（源码符号）与 `structured_doc/`（6 类文档：commit / ingested / issue / page / pull / web）。 |

### J

| 术语 | 定义 |
|------|------|
| **Juniper** | Rust GraphQL server library，EE 用它声明并实现全部 schema。 |

### L

| 术语 | 定义 |
|------|------|
| **LDAP** | EE 支持的目录服务认证。真实字段（`ldap_credential` 单行表 + `UpdateLdapCredentialInput`）：`host`、`port`（列默认 389）、`bind_dn`、`bind_password`、`base_dn`、`user_filter`、`encryption`（`none` / `starttls` / `ldaps`）、`skip_tls_verify`、`email_attribute`（默认 `email`）、`name_attribute`。**没有** `search_base` / `search_filter` / `bind_dn_password` 这三个名字。 |
| **License** | EE 许可管理。档位只有三档：`LicenseType { Community, Team, Enterprise }`（**不是** Free/Pro/Enterprise），座位上限 5 / 50 / 无限，管理员上限 1 / 3 / 无限；license 是一串 JWT 原文，存在 `server_setting.billing_enterprise_license` 列里，用编译期烧进去的 RSA 公钥验签。详见第 12 章 §12.7。 |
| **LLM Context** | Prompt 里附带的外部信息（代码片段、声明、文件路径等），来源包括 Tantivy code search、LSP declaration、最近修改的代码。 |

### M

| 术语 | 定义 |
|------|------|
| **Metal** | Apple 的 GPU API。注意 `crates/tabby/Cargo.toml` 只有 `cuda` / `rocm` / `vulkan` 三个 device feature，**没有 `metal` 这一档** —— macOS 上由 llama.cpp 二进制自行启用 Metal。 |

### O

| 术语 | 定义 |
|------|------|
| **OAuth** | EE 支持 GitHub / Google / GitLab / OIDC 四种 provider（`OAuthProvider` 枚举），代码在 `ee/tabby-webserver/src/oauth/`，凭证统一存 `oauth_credential` 表、以 `provider` 为业务主键。回调地址由 `network_setting.external_url` 拼出 `{external_url}/oauth/callback/{provider}`，**不是**可配字段。 |

### P

| 术语 | 定义 |
|------|------|
| **Postprocessor** | tabby-agent 对补全候选做过滤/变换的规则集，位于 `clients/tabby-agent/src/codeCompletion/postprocess/`：**13 个规则文件**（dropDuplicated、dropMinimum、formatIndentation、limitScope、limitScopeByIndentation、normalizeIndentation、removeDuplicateSuffixLines、removeDuplicatedBlockClosingLine、removeLineEndsWithRepetition、removeRepetitiveBlocks、removeRepetitiveLines、trimMultiLineInSingleLineMode、trimSpace）。默认两条链：`preCacheProcess` 串 5 个、`postCacheProcess` 串 10 个（合计 12 个不重复过滤器；`limitScopeByIndentation` 不在这两条链上）。 |
| **PR (Pull Request)** | `PullDocument { link, title, author_email, body, diff, merged }`（`crates/tabby-index/src/structured_doc/types/pull.rs`）；`diff` 只在 ≤1MB 时存属性，且**当前不进 chunk 索引**（`build_chunk_attributes` 只用 title + body）。没有 `changed_files` 字段。用途是 Answer Engine 检索，不是补全上下文。 |

### R

| 术语 | 定义 |
|------|------|
| **RAG (Retrieval-Augmented Generation)** | 先检索再生成：Answer Engine 从 Tantivy 取 top-K 片段拼进 prompt。 |
| **RBAC (Role-Based Access Control)** | EE 的访问控制：`users` / `user_groups` / `user_group_memberships` / `source_id_read_access_policies` 四张表构成「用户 → 组 → 可读 source」链路。 |

### S

| 术语 | 定义 |
|------|------|
| **Service Locator** | EE 的依赖注入约定：`ServiceLocator` trait（`ee/tabby-schema/src/schema/mod.rs`）暴露 **23 个服务访问器**，名字就是这 23 个：`auth`、`worker`、`code`、`chat`、`completion`、`embedding`、`logger`、`ingestion`、`job`、`repository`、`integration`、`email`、`setting`、`license`、`analytic`、`user_event`、`web_documents`、`thread`、`page`、`context`、`user_group`、`access_policy`、`notification`。GraphQL resolver 与 background job 都通过它取服务（没有 `webSearch()` / `analytics()` 这种名字）。 |
| **SSE (Server-Sent Events)** | 单向流式推送协议，chat 与 completion 的 token-by-token 输出都走它。 |
| **SQLx** | 异步 SQL toolkit。`ee/tabby-db` 用它连 SQLite（WAL），migration 文件放在 `ee/tabby-db/migrations/`，并用编译期宏 `query!` / `query_as!` 做 SQL 校验。 |

### T

| 术语 | 定义 |
|------|------|
| **Tantivy** | Rust 全文搜索引擎（对标 Lucene）。Tabby 用 `code/` 与 `structured_doc/` 两类 schema 建索引，做 BM25 + embedding 双路检索。 |
| **tree-sitter** | 增量解析库。`crates/tabby-index/Cargo.toml` 引入 15 个 `tree-sitter-*` grammar；`LANGUAGE_TAGS` 的 15 个 key 是 python、rust、java、scala、kotlin、javascript-typescript、elixir、go、ruby、c、cpp、csharp、solidity、lua、gdscript。其余语言走纯文本切分（可通过 `additional_languages` 补 `line_comment` / `top_level_keywords` 来改善 stop words，但**不能**注册新 grammar）。 |
| **TABBY_ROOT** | server 数据根目录环境变量，默认 `~/.tabby`，Docker 里约定 `/data`。真实子路径（`crates/tabby-common/src/path.rs`）：`config.toml`、`usage_anonymous_id`、`repositories/`（**不是** `repos/`）、`index/`、`models/`、`events/`；EE 的数据库是 `ee/db.sqlite`（dev 构建为 `dev-db.sqlite`，见 `ee/tabby-webserver/src/path.rs · db_file()`），**没有 `db/` 目录**。 |
| **TRUNCATE (WAL Checkpoint)** | SQLite 的 `PRAGMA wal_checkpoint(TRUNCATE)`，把 WAL 内容折回主库文件并截断 WAL —— 做冷备份前常用；Tabby 自身没有对它做定时任务（`DbMaintenanceJob` 只做行级清理）。 |

### V

| 术语 | 定义 |
|------|------|
| **Vulkan** | 跨平台 GPU API，通过 `--features vulkan` 编译开关启用（与 `cuda` / `rocm` 并列，见 `crates/tabby/Cargo.toml`）。 |

### W

| 术语 | 定义 |
|------|------|
| **Web Page** | EE 中可持久化、可分节的回答页面，存 `pages` + `page_sections` 两张表。**没有 `createPage` mutation** —— 页面内容由 subscription 产生（`createPageRun` / `createThreadToPageRun` / `createPageSectionRun`），mutation 侧只有 `updatePageTitle` / `updatePageContent` / `updatePageSectionTitle` / `updatePageSectionContent` / `deletePage` / `deletePageSection`。 |
| **WAL (Write-Ahead Logging)** | SQLite 预写日志模式，EE 数据库启用，允许读写并发；副作用是多出一个 `db.sqlite-wal` 文件，直接复制主库文件可能拿不到最新事务 —— 备份请用 `sqlite3 .backup` 或先做 checkpoint（见第 8 章 §8.9）。 |
