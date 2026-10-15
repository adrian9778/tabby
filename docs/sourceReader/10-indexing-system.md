# 第十章：索引系统

## 10.1 Indexing Architecture Overview

Tabby 使用 **Tantivy**（Rust 编写的全文搜索引擎）作为底层索引引擎，构建双路检索管道：

```
┌────────────────────── Code Indexing ──────────────────────┐
│                                                            │
│  Git Repos → tree-sitter AST Parsing → Symbol Extraction  │
│              ↓                                               │
│         Tantivy Inverted Index (corpus = "code")           │
│         (chunk fields: chunk_filepath, chunk_language,      │
│          chunk_body, chunk_start_line)                      │
│                                                            │
└────────────────────────────────────────────────────────────┘

┌────────────── Structured Document Indexing ────────────────┐
│                                                             │
│  Issues/Pulls/Commits/WebPages → Text Normalization        │
│             ↓                                                │
│      Tantivy Inverted Index (corpus = "structured_doc")     │
│      (kind-specific fields: issue.{title,link,body}, …)     │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Core Components

| Component | 职责 | Crate / 位置 |
|-----------|------|-------|
| **Indexer** | Orchestrate indexing pipeline | `crates/tabby-index`（`crates/tabby-index/src/indexer.rs`、`crates/tabby-index/src/tantivy_utils.rs`） |
| **Code Scanner** | tree-sitter AST parsing | `crates/tabby-index/src/code/` |
| **Doc Indexer** | Structured doc schema | `crates/tabby-index/src/structured_doc/` |
| **Search API** | reader 装载 + 检索实现 | `crates/tabby/src/services/tantivy.rs` · `IndexReaderProvider`；`crates/tabby/src/services/code.rs` · `CodeSearchService`；`crates/tabby/src/services/structured_doc/tantivy.rs` |

注意：code 与 structured doc **共用同一个 Tantivy 索引目录**（`path::index_dir()`），靠 `corpus` 字段区分（`crates/tabby-common/src/index/mod.rs` · `corpus::CODE` / `corpus::STRUCTURED_DOC`）。

---

## 10.2 Code Indexing Pipeline

### Step-by-step Process

```
Step 1: Repository Discovery
─────────────────────────────
Config.toml [repositories] or Admin UI → Git clone/pull
  → tabby-index clones repos to ~/.tabby/repositories/{sanitize(canonical git url)}/
    (path::repositories_dir() + config.rs · resolve_dir_name)

Step 2: File Enumeration
─────────────────────────────
Walk all files under repository.dir() with the `ignore` crate
  (crates/tabby-index/src/code/index.rs · Walk::new；按 gitignore 语义遍历，并非硬编码排除清单)
  → List of file paths + extensions

Step 3: Language Detection & AST Parsing
────────────────────────────────────────
For each file:
  - Match extension → tree-sitter grammar (实测 15 个 language key，见 10.3)
  - Parse → Abstract Syntax Tree (AST)
  - Extract: function/class/struct/enum/variable symbols

Step 4: Intelligence Analysis
─────────────────────────────
- Import/dependency graph (import → source mapping)
- Symbol scope analysis (local/global/namespace)
- Cross-file reference tracking

Step 5: Index Writing
─────────────────────────────
For each symbol + file segment (chunk):
  - Write to the shared Tantivy index (corpus = "code")
  - Doc-level: source_id / id / updated_at / attributes{commit}
  - Chunk-level: chunk_git_url / chunk_filepath / chunk_language /
    chunk_body / chunk_start_line + chunk_tokens（分词倒排）
```

### Tantivy Index Schema (Code)

实测字段定义在 `crates/tabby-common/src/index/code/mod.rs` · `fields`（chunk 级属性以 JSON 存于 `chunk_attributes`，检索 token 存于 `chunk_tokens`）：

| Field | 级别 | 用途 |
|-------|------|------|
| `commit` | Doc | 被索引文件所属的 commit ref |
| `chunk_git_url` | Chunk | 可直接跳转的源码 URL（含行号区间） |
| `chunk_filepath` | Chunk | 相对仓库根的文件路径（精确过滤） |
| `chunk_language` | Chunk | 语言过滤（javascript/typescript 归并为 `javascript-typescript`） |
| `chunk_body` | Chunk | 符号/代码段内容（BM25 分词检索源） |
| `chunk_start_line` | Chunk | 起始行；缺省表示该 chunk 覆盖整个文件 |

---

## 10.3 Language Support

### Built-in Languages (tree-sitter grammars)

Tabby 内置 **15 个** tree-sitter language key（实测 `crates/tabby-index/src/code/languages.rs` · `LANGUAGE_TAGS` 的 HashMap key）：

| 语言 | 文件扩展名 | AST Node Types Extracted |
|------|-----------|-------------------------|
| Python | `.py` | function, class, method, variable, decorator |
| Rust | `.rs` | function, struct, impl, trait, enum |
| TypeScript/TSX + JavaScript/JSX | `.ts`, `.tsx`, `.js`, `.jsx` | 共用 `javascript-typescript` key（tsx grammar + `queries/tsx.scm`）：function, interface, class, export |
| Go | `.go` | function, struct, method, interface |
| Java | `.java` | class, method, field, interface |
| C | `.c`, `.h` | function, struct, typedef, enum |
| C++ | `.cpp`, `.hpp` | function, class, template |
| C# | `.cs` | class, method, property, interface |
| Ruby | `.rb` | method, class, module |
| Scala | `.scala` | class, object, method, trait |
| Kotlin | `.kt`, `.kts` | class, function, interface, data class |
| Solidity | `.sol` | contract, function, modifier |
| Lua | `.lua` | function, table, class |
| GDScript | `.gd` | func, class, signal, enum |
| Elixir | `.ex`, `.exs` | def, defp, module, protocol |

### Custom Languages (`additional_languages` config)

实测：`Config.additional_languages` 的类型是 `Vec<languages::Language>`（`crates/tabby-common/src/languages.rs` · `Language`），字段是 `languages / exts / top_level_keywords / line_comment / chunk_size`——它只影响补全侧的停止词、行注释与 chunk 大小，**不能**注册新的 tree-sitter grammar（grammar 集合硬编码在 `crates/tabby-index/src/code/languages.rs`）。旧文档写的 `name/file_extensions/grammar = "github://.../grammar.json"` 配置项不存在。

```toml
# config.toml（字段按实测；示例语言名）
[[additional_languages]]
languages = ["mylang"]
exts = ["myc"]
line_comment = "//"
chunk_size = 128
```

---

## 10.4 Structured Document Indexing

### Document Types and Sources

按实测 kind 共 6 类（`crates/tabby-common/src/index/structured_doc.rs` · `fields`；kind 常量在同文件 `KIND_*`）：

| Type (kind) | Source | 实际索引字段（doc 级 / chunk 级） |
|------|--------|-------------------|
| **commit** | Git commit log | sha, message, author_email, author_at（doc 级） |
| **issue** | GitHub/GitLab API | title, link, author_email, body, closed |
| **pull** | GitHub/GitLab API | title, link, author_email, body, diff, merged |
| **web** | 预置/抓取 web 文档 | title, link, chunk_text |
| **page** | Admin 自建页面 | link, title（doc 级）+ chunk_text（chunk 级） |
| **ingested** | ingestion API 导入 | title, link（doc 级）+ chunk_body（chunk 级） |

### Index Schema (Structured Doc)

```rust
// crates/tabby-index/src/structured_doc/types.rs · `StructuredDocFields`（实测）
pub enum StructuredDocFields {
    Web(web::WebDocument),
    Issue(issue::IssueDocument),
    Pull(pull::PullDocument),
    Commit(commit::CommitDocument),
    Page(page::PageDocument),
    Ingested(ingested::IngestedDocument),
}

// 所有文档共用同一套索引骨架（crates/tabby-common/src/index/mod.rs · `IndexSchema`）：
//   doc 级   : corpus / source_id / id / updated_at / attributes(JSON) / failed_chunks_count
//   chunk 级 : chunk_id / chunk_attributes(JSON) / chunk_tokens(倒排 token)
// 各 kind 的 title/body/link 等不是独立 schema 字段，而是放进 attributes / chunk_attributes
// 的 JSON 子字段（字段名见上表；查询构造见 crates/tabby-common/src/index/structured_doc.rs）
```

### RAG Retrieval for Chat (Answer Engine)

当用户发送聊天消息时，Tantivy 执行多路召回：

```
User Query → Multi-Modal Retrieval Pipeline
    │
    ├── Path A: Code Search
    │   ├── BM25 keyword search（chunk_tokens，即 chunk_body 的分词倒排）
    │   └── Embedding vector similarity (if embedding model loaded)
    │
    ├── Path B: Document Search
    │   ├── BM25 on title/chunk body（chunk_attributes + chunk_tokens）
    │   └── Filter by kind/source_id if specified
    │
    ├── Path C: Repository Context
    │   ├── LSP declarations from current file
    │   └── Recently modified code snippets
    │
    └── Rank Fusion (RRF: Reciprocal Rank Fusion)
        → Merge A, B, C results by RRF score
        → Top-K results returned as context to LLM
```

### Search Parameters (`AnswerConfig`)

| 参数 | 默认值 | 含义 |
|------|--------|------|
| `min_embedding_score` | 0.5 | embedding 最小相似度阈值 |
| `min_bm25_score` | -1.0 | BM25 最低分（-1 = 不限制） |
| `min_rrf_score` | -1.0 | RRF fusion 最低合并分 |
| `num_to_return` | 10 | 最终返回的 top-K 结果数 |
| `num_to_score` | 100 | 参与评分的候选集大小 |

---

## 10.5 Index CLI (`tabby-index-cli`)

### Available Commands

实测 `crates/tabby-index-cli/src/main.rs` · `Commands` 只有三个子命令 `inspect / bench / head`（旧文档写的 `tabby index list/rebuild/delete/gc/health` 不存在；重建索引由 `tabby scheduler` 完成，见第 12 章）：

```bash
# 全局参数 -i/--index-dir 覆盖索引目录（默认 path::index_dir()）

# 打印索引结构统计
cargo run -p tabby-index-cli -- inspect

# 展示前 N 个文档（-n 默认 1，-c 选 corpus：code / structured_doc）
cargo run -p tabby-index-cli -- head -n 3 -c code

# 用查询文件回放检索并计时（-q 查询文件，-r 重复次数）
cargo run -p tabby-index-cli -- bench -q queries.txt -r 3
```

---

## 10.6 Background Indexing Jobs

### EE Cron Jobs (in `ee/tabby-webserver/src/service/background_job/`)

实测调度入口在 `ee/tabby-webserver/src/service/background_job/mod.rs`（`@hourly` / `@daily` / `*/10 * * * * *` 三个 CronStream）；旧文档写的 `service/job/` 目录不存在：

| Job File | Schedule（实测） | Function |
|----------|---------|----------|
| `index_commits.rs` | 随 hourly 事件：`SchedulerGitJob`（`git.rs`）逐 repo pull 后调 `index_commits::refresh` | commit 索引构建/刷新 |
| `index_pages.rs` | `SyncPageIndexJob::cron` 在 hourly 事件里触发一次（`mod.rs`） | 自建 page 文档索引 |
| `index_ingestion.rs` | `SyncIngestionIndexJob::cron` 每 10 秒检查一次待导入文档（`*/10 * * * * *`） | ingestion 导入文档索引 |
| `index_garbage_collection.rs` | `IndexGarbageCollection` 在 HourlyJob 内执行（hourly，非 daily） | 清理已删除仓库/文档的索引条目 |

### Commit Indexing Flow (detailed)

```
New commits detected (hourly SchedulerGitJob 先 pull，见 ee/tabby-webserver/src/service/background_job/git.rs)
    → index_commits::refresh(repository)
    → 枚举待索引 commits（携带 sha/author/message 等 doc 级字段）
    → 按 document id upsert 到共享 Tantivy 索引（corpus = structured_doc, kind = commit）
    → 没有单独的「索引进度表」：进度以每次 refresh 重新对账的方式体现
```

### Web Page Crawling Flow

```
URL added to ingestion queue (`ee/tabby-db/src/ingestion.rs` 的 ingestion 表)
    → SyncIngestionIndexJob 每 10s 检查 pending（ee/tabby-webserver/src/service/background_job/index_ingestion.rs）
    → 按 source 拉取内容，normalize + chunking
    → Write to Tantivy index (corpus = structured_doc, kind = ingested)
    → 更新 tabby-db 中该 source 的 pending 状态
```

---

## 10.7 Search API & Retrieval

### Code Search（实测结构）

```rust
// crates/tabby/src/services/tantivy.rs · `IndexReaderProvider`（实测源码）
// 只负责 reader：后台循环 Index::open_in_dir(path::index_dir())，
// 校验 IndexSchema::instance().schema 匹配后写入 RwLock<Option<IndexReader>>；
// 索引未就绪时检索方拿到 None（旧文档的 TantivyIndexProvider / index+searcher 双字段结构不存在）
pub struct IndexReaderProvider {
    provider: Arc<RwLock<Option<IndexReader>>>,
    loader: tokio::task::JoinHandle<()>,
}

// 真正的 CodeSearch 实现在 crates/tabby/src/services/code.rs · `CodeSearchService`：
impl CodeSearch for CodeSearchService {
    async fn search_in_language(
        &self,
        query: CodeSearchQuery,
        params: CodeSearchParams,
    ) -> Result<CodeSearchResponse, CodeSearchError> {
        // reader 未就绪 → CodeSearchError::NotReady
        // BM25(chunk_tokens) + embedding 相似度 + RRF 融合，
        // 查询构造见 crates/tabby-common/src/index/code/mod.rs · code_search_query
    }
}
```

### Code Search Params (`CodeSearchParams`)

结构定义在 `crates/tabby-common/src/api/code.rs` · `CodeSearchParams`。注意有两套默认值：

| 参数 | 内置 Default（completion 配置用） | `answer.code_search_params` 默认 |
|------|--------|------|
| `min_embedding_score` | 0.75 | 0.5 |
| `min_bm25_score` | 8.0 | -1.0（不限制） |
| `min_rrf_score` | 0.028 | -1.0 |
| `num_to_return` | 20 | 10 |
| `num_to_score` | 40 | 100 |

（内置值见 `CodeSearchParams::default`；answer 侧见 `crates/tabby-common/src/config.rs` · `default_answer_code_search_params`，`CompletionConfig` 则直接用内置 Default。）

---

## 10.8 Index Storage Layout

### File System Structure

按 `crates/tabby-common/src/path.rs` 与 `ee/tabby-webserver/src/path.rs` 实测（code 与 structured doc 共用一个 `index/` 目录，旧文档画的 `index/code/tantivy/`、`index/docs/tantivy/` 分离结构不存在；`repos/`、`db/default.sqlite` 也非实际名称）：

```
~/.tabby/                          # TABBY_ROOT（env 可覆盖）
├── config.toml
├── usage_anonymous_id
├── repositories/                  # 按 sanitize(canonical git url) 克隆的仓库
├── models/                        # GGUF 模型缓存（TABBY_MODEL_CACHE_ROOT 可重定向）
├── events/                        # completion/chat 事件 jsonl（日切）
├── index/                         # 唯一的 Tantivy 索引目录（path::index_dir()）
│   └── meta.json / segments_*     # Tantivy 运行期生成，仓库源码中不存在该文件
└── ee/                            # EE 数据目录（tabby_ee_root()）
    ├── db.sqlite                  # prod feature；dev 构建为 dev-db.sqlite
    └── jobs/                      # background job 记录
```

### Index Size Estimation

| Repo Size | Approximate Index Size |
|-----------|----------------------|
| 10K SLoC | ~5 MB |
| 100K SLoC | ~50 MB |
| 1M SLoC | ~500 MB |
| 10M+ SLoC | ~5 GB |
