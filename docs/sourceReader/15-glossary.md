# 第十五章：术语表

## Glossary

以下按字母顺序列出 Tabby 项目中所有关键术语和概念的定义。

---

### A

| 术语 | 定义 |
|------|------|
| **Access Policy** | EE 中控制用户/用户组对特定资源（仓库、页面、设置等）访问权限的 RBAC 规则。存储于 `_access_policies` 表。 |
| **Agent (tabby-agent)** | 运行在 IDE 进程内的 TypeScript Agent，负责与 Tabby server 通信、构建补全请求、管理 postprocessing pipeline 和 chat 交互。 |
| **Answer Engine** | EE 中的 RAG 对话引擎：结合代码搜索、文档索引和用户问题，将检索结果作为上下文发送给聊天模型，生成精确回答。 |
| **AST (Abstract Syntax Tree)** | 源码的树形结构表示。Tabby 使用 tree-sitter 为每种语言生成 AST，用于提取符号（函数/类/方法）和作用域信息。 |
| **Axum** | Rust 编写的 HTTP web 框架，作为 Tabby server 的底层 HTTP 路由和请求处理引擎。 |

### B

| 术语 | 定义 |
|------|------|
| **BM25** | Boolean Matching with BM25 — Tantivy 中使用的文本匹配算法。Tabby 对代码和文档文本执行 BM25 keyword search，与 embedding vector similarity 结合使用。 |
| **CUDA** | NVIDIA 的并行计算平台和编程模型。Tabby 通过 llama-cpp-server crate 管理 llama.cpp CUDA GPU 推理进程。 |

### C

| 术语 | 定义 |
|------|------|
| **Completion Pipeline** | Tabby IDE → Agent → Server 端的完整代码补全处理链：context capture → prompt building → model inference → postprocessing（20+ rules）→ inline display。 |
| **Config.toml** | Tabby 的 YAML/TOML 配置文件，存储于 `~/.tabby/config.toml`。包含 model.completion、model.chat、model.embedding、repositories、server、completion、answer 等配置节。 |
| **Context Provider** | tabby-agent 中的模块（git / workspace / document / lsp），负责收集 IDE 中各种上下文信息并增强发送给 server 的请求。 |
| **CronScheduler** | `tokio-cron-scheduler` crate 实现的定时任务调度器。在 EE 中管理 daily/hourly/indexing/crawler/license_check 等 background jobs。 |

### D

| 术语 | 定义 |
|------|------|
| **FIM (Fill-in-the-Middle)** | 一种 LLM 推理模式：给定 prefix + suffix，模型预测中间缺失的文本。Tabby 的所有代码补全均基于 FIM。prompt_template（如 `<PRE>{prefix}<SUF>{suffix}<MID>`）指定了 FIM 格式。 |
| **Fusion (RRF)** | Reciprocal Rank Fusion — 将 BM25 keyword search 和 embedding vector similarity 两种检索策略的 ranking 结果合并为最终排序的融合算法。 |

### G

| 术语 | 定义 |
|------|------|
| **GGUF** | GGML Unified Format — llama.cpp 使用的模型权重文件格式。量化等级包括 FP16、Q8_0、Q6_K、Q5_K_M、Q4_K_M、Q3_K_M。Tabby 通过 `llama-cpp-server` crate 加载 GGUF 文件。 |
| **GraphQL** | Facebook 开发的一种查询语言。EE（企业版）通过 Juniper crate 暴露 GraphQL API（~60 Queries, ~45 Mutations, ~4 Subscriptions）。 |
| **Juniper** | Rust 编写的 GraphQL server library。Tabby EE 使用 Juniper 定义和实现所有 GraphQL schema（Query/Mutation/Subscription）。 |
| **JWT (JSON Web Token)** | 用户身份认证令牌。EE 的 `/tokenAuth` mutation 返回 JWT access token（TTL: 24h）和 refresh token（TTL: 30d）。 |

### I

| 术语 | 定义 |
|------|------|
| **Ingestion** | Tabby 将自定义文档（URL、网页、GitHub Issues/Pulls）通过 crawler 采集、解析、normalization、chunking 后，写入 Tantivy 索引的完整流程。 |
| **Integration (GitHub/GitLab)** | EE 中的第三方代码平台集成：通过 OAuth API 同步 GitHub PR/Issue 或 GitLab MR/Commit 到 Tabby 的索引系统中。 |
| **Index** | Tantivy inverted index — 倒排索引数据结构。Tabby 维护两个独立索引：code index（源码符号）和 docs index（结构化文档）。 |

### L

| 术语 | 定义 |
|------|------|
| **LDAP (Lightweight Directory Access Protocol)** | EE 中支持的目录服务协议，用于企业级用户认证。配置项包括 host、port、bind_dn、search_base、search_filter。 |
| **LLM Context** | Prompt 中提供给 LLM 的额外信息（代码片段、导入关系、文件路径），用于增强补全和对话的上下文相关性。来源包括 Tantivy code search、LSP declarations、recently modified code。 |
| **License (Free/Pro/Enterprise)** | EE 许可证管理：Free tier（仅开源 API）、Pro tier（GraphQL + RBAC + Analytics）、Enterprise tier（全部功能 + LDAP + custom license）。通过 `ee/tabby-webserver/src/service/license.rs` 管理。 |

### M

| 术语 | 定义 |
|------|------|
| **Metal** | Apple 的跨平台 GPU 计算 API。M1/M2 Mac 上通过 Metal backend 运行 llama.cpp，无需额外安装。 |
| **Postprocessor** | tabby-agent 中对 completion candidate 结果进行过滤/变换的规则集（共 20+ 条）：trimSpace、limitScope、normalizeIndentation、removeDuplicateSuffixLines、dropMinimum、removeRepetitiveBlocks 等。 |

### O

| 术语 | 定义 |
|------|------|
| **OAuth (Open Authorization)** | EE 中支持的外部认证协议。包括 GitHub、GitLab、Google OAuth2 和通用 OIDC 四种 provider 实现，位于 `ee/tabby-webserver/oauth/`。 |

### P

| 术语 | 定义 |
|------|------|
| **PR (Pull Request)** | GitHub/GitLab 的代码合并请求。Tabby 将 PR 的 title、body、diff、changed_files 索引入 Tantivy，用于 code completion context 和 Answer Engine RAG 检索。 |
| **RAG (Retrieval-Augmented Generation)** | 检索增强生成 — Answer Engine 的核心技术：先通过 Tantivy 搜索代码/文档得到 top-K 相关片段，再将它们作为 system prompt 的上下文发送给 LLM。 |
| **RBAC (Role-Based Access Control)** | EE 中的基于角色的访问控制。核心概念包括 User、UserGroup、AccessPolicy。通过 `ee/tabby-webserver/src/service/access_policy.rs` + `user_group.rs` 实现。 |
| **Service Locator** | EE 中统一管理服务实例的接口模式 (`ServiceLocator` trait)。管理约 25 个内部服务（auth、worker、code、chat、embedding、logger、license、analytics 等）。 |

### S

| 术语 | 定义 |
|------|------|
| **SSE (Server-Sent Events)** | HTTP 服务器向客户端推送实时数据流的协议。Tabby 使用 SSE streaming 实现 chat completion 的 token-by-token 流式响应。 |
| **SQLx** | Rust 编写的异步 SQL toolkit。EE 中 tabby-db crate 使用 SQLx 连接 SQLite（WAL mode），并通过 `sqlx-migrate-validate` 在编译时验证 migration 文件。 |
| **Service Locator Pattern** | EE 中的依赖注入模式：通过 `ServiceLocator` trait 统一管理所有内部服务的生命周期，解耦服务间的直接依赖。 |
| **Tantivy** | Rust 编写的全文搜索引擎（类似 Lucene）。Tabby 使用 Tantivy 构建 code index 和 structured_doc index，实现 BM25 keyword search 和 embedding similarity search。 |
| **tree-sitter** | C/Rust 编写的增量解析库，支持 14+ 编程语言的 grammar。Tabby 通过 tree-sitter AST 提取代码符号（函数/类/变量）和作用域信息，用于 code indexing 和 context enrichment。 |

### T

| 术语 | 定义 |
|------|------|
| **TABBY_ROOT** | Tabby 服务器的主数据目录环境变量，默认 `~/.tabby`。包含 models/、repos/、db/、index/ 等子目录。Docker 中映射为 `/data`。 |
| **TRUNCATE (WAL Checkpoint)** | SQLite WAL mode 中的 checkpoint 操作（`PRAGMA wal_checkpoint(TRUNCATE)`），将 WAL 日志中的数据刷入主数据库文件，保证一致性备份。 |

### V

| 术语 | 定义 |
|------|------|
| **Vulkan** | 跨平台 GPU 加速 API。Tabby 通过 `--features vulkan` 编译 flag 启用 Vulkan backend。支持 Linux/Windows/macOS 上的 GPU 推理。 |

### W

| 术语 | 定义 |
|------|------|
| **Web Page** | EE 中 Answer Engine 产生的持久化内容页面。存储在 `_pages` 表中，可通过 GraphQL `createPage` mutation 或 chat subscription 创建。 |
| **WAL (Write-Ahead Logging)** | SQLite 的预写日志模式。Tabby DB 默认启用 WAL mode，允许读写并发（读不阻塞写），提升高并发场景下的性能。 |
