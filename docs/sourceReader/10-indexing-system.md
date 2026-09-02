# 第十章：索引系统

## 10.1 Indexing Architecture Overview

Tabby 使用 **Tantivy**（Rust 编写的全文搜索引擎）作为底层索引引擎，构建双路检索管道：

```
┌────────────────────── Code Indexing ──────────────────────┐
│                                                            │
│  Git Repos → tree-sitter AST Parsing → Symbol Extraction  │
│              ↓                                               │
│         Tantivy Inverted Index                              │
│         (fields: file_path, symbol_name, code_text)         │
│                                                            │
└────────────────────────────────────────────────────────────┘

┌────────────── Structured Document Indexing ────────────────┐
│                                                             │
│  Issues/Pulls/Commits/WebPages → Text Normalization        │
│             ↓                                                │
│      Tantivy Inverted Index                                 │
│      (fields: title, content, source_type, repo)            │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Core Components

| Component | 职责 | Crate |
|-----------|------|-------|
| **Indexer** | Orchestrate indexing pipeline | `tabby-index` |
| **Code Scanner** | tree-sitter AST parsing | `tabby-index/code/` |
| **Doc Indexer** | Structured doc schema | `tabby-index/structured_doc/` |
| **Search API** | Tantivy searcher interface | `crates/tabby/services/tantivy.rs` |

---

## 10.2 Code Indexing Pipeline

### Step-by-step Process

```
Step 1: Repository Discovery
─────────────────────────────
Config.toml [repositories] or Admin UI → Git clone/pull
  → tabby-index clones repos to ~/.tabby/repos/{sanitized_url}/

Step 2: File Enumeration
─────────────────────────────
Walk all source files (excludes: node_modules, vendor, .git, *.min.*, etc.)
  → List of file paths + extensions

Step 3: Language Detection & AST Parsing
────────────────────────────────────────
For each file:
  - Match extension → tree-sitter grammar (14+ languages)
  - Parse → Abstract Syntax Tree (AST)
  - Extract: function/class/struct/enum/variable symbols

Step 4: Intelligence Analysis
─────────────────────────────
- Import/dependency graph (import → source mapping)
- Symbol scope analysis (local/global/namespace)
- Cross-file reference tracking

Step 5: Index Writing
─────────────────────────────
For each symbol + file segment:
  - Write to Tantivy inverted index
  - Fields: file_path, line_start, line_end, code_snippet, language, symbol_type, scope_info
```

### tanty Index Schema (Code)

| Field | Type | 用途 |
|-------|------|------|
| `file_path` | TEXT | Source file path (relative to repo root) |
| `line_start` | INTEGER | Symbol start line |
| `line_end` | INTEGER | Symbol end line |
| `code_text` | TEXT | Code content (tokenized for BM25 search) |
| `language` | TEXT | Language name (filter by language) |
| `symbol_type` | TEXT | Function/class/struct/etc. |
| `scope_info` | JSON | Import scope, namespace hierarchy |

---

## 10.3 Language Support

### Built-in Languages (tree-sitter grammars)

Tabby 支持 **14+ 语言**的 AST 解析和符号提取：

| 语言 | 文件扩展名 | AST Node Types Extracted |
|------|-----------|-------------------------|
| Python | `.py` | function, class, method, variable, decorator |
| Rust | `.rs` | function, struct, impl, trait, enum |
| TypeScript/TSX | `.ts`, `.tsx` | function, interface, class, export |
| JavaScript/JSX | `.js`, `.jsx` | function, class, const/let/var |
| Go | `.go` | function, struct, method, interface |
| Java | `.java` | class, method, field, interface |
| C/C++ | `.c`, `.h`, `.cpp`, `.hpp` | function, struct, typedef, enum |
| C# | `.cs` | class, method, property, interface |
| Ruby | `.rb` | method, class, module |
| Scala | `.scala` | class, object, method, trait |
| Kotlin | `.kt`, `.kts` | class, function, interface, data class |
| Solidity | `.sol` | contract, function, modifier |
| Lua | `.lua` | function, table, class |
| GDScript | `.gd` | func, class, signal, enum |
| Elixir | `.ex`, `.exs` | def, defp, module, protocol |

### Custom Languages (`additional_languages` config)

```toml
# config.toml — Add custom language support
[[additional_languages]]
name = "my-custom-lang"
file_extensions = [".myc"]
grammar = "github://username/repo@main/path/to/grammar.json"
```

---

## 10.4 Structured Document Indexing

### Document Types and Sources

| Type | Source | Key Fields Indexed |
|------|--------|-------------------|
| **Commit** | Git commit log | message, added_lines, removed_lines, author |
| **Issue** | GitHub/GitLab API | title, body, labels, comments |
| **Pull Request** | GitHub/GitLab API | title, body, diff, changed_files |
| **Web Page** | Web crawler (llms.txt) | title, markdown_content, source_url |
| **Custom Document** | Direct upload/API | name, content_type, raw_text |

### Index Schema (Structured Doc)

```rust
pub enum StructuredDocument {
    Commit(CommitDocument),      // git commit entry
    Issue(IssueDocument),         // GitHub/GitLab issue
    Pull(PullDocument),           // PR/MR with diff
    Page(PageDocument),           // Crawled web page
    Web(WebDocument),             // Custom document
}

// All types share common Tantivy fields:
pub struct CommonFields {
    pub id: String,              // Unique identifier
    pub title: String,           // Title (BM25 searchable)
    pub content: String,         // Full text (BM25 + embedding)
    pub source_type: String,     // "commit" | "issue" | "pull" | "page"
    pub source_id: String,       // Original ID from source system
    pub repo: Option<String>,    // Associated repository URL
    pub created_at: u64,         // Timestamp
}
```

### RAG Retrieval for Chat (Answer Engine)

当用户发送聊天消息时，Tantivy 执行多路召回：

```
User Query → Multi-Modal Retrieval Pipeline
    │
    ├── Path A: Code Search
    │   ├── BM25 keyword search on code_text field
    │   └── Embedding vector similarity (if embedding model loaded)
    │
    ├── Path B: Document Search
    │   ├── BM25 on title + content
    │   └── Filter by source_type if specified
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

```bash
# List current index info
tabby index list

# Rebuild code index for all repos
tabby index rebuild --type code

# Rebuild document index
tabby index rebuild --type docs

# Delete (clear) entire index
tabby index delete

# Index garbage collection (remove stale entries)
tabby index gc

# Check index health
tabby index health
```

---

## 10.6 Background Indexing Jobs

### EE Cron Jobs (in `ee/tabby-webserver/src/service/job/`)

| Job File | Schedule | Function |
|----------|---------|----------|
| `index_commits.rs` | Hourly | Index new commits from registered repos |
| `index_pages.rs` | Hourly | Index crawled web pages and custom docs |
| `index_ingestion.rs` | Every 15min | Process pending ingestion queue items |
| `index_garbage_collection.rs` | Daily | Clean up orphaned index entries |

### Commit Indexing Flow (detailed)

```
New commits detected (git fetch origin)
    → Diff between old/new HEAD
    → For each changed file:
        1. Parse with tree-sitter
        2. Extract new/modified symbols
        3. Update Tantivy index (upsert by line number)
    → Update commit index entry (message + diff stats)
    → Mark as indexed in _commit_indexing_status table
```

### Web Page Crawling Flow

```
URL added to ingestion queue (_ingestion table)
    → Cron job picks up pending URL
    → HTTP GET → Content extraction (readability algorithm)
    → Parse llms.txt (if exists at root of URL domain)
    → For each linked doc: crawl recursively
    → Text normalization + chunking
    → Write to Tantivy index
    → Update _ingestion status = "done" + indexed_at timestamp
```

---

## 10.7 Search API & Retrieval

### Code Search (Tantivy Query)

```rust
// crates/tabby/src/services/tantivy.rs
pub struct TantivyIndexProvider {
    index: tantivy::Index,        // The Tantivy index
    searcher: tantivy::Searcher,  // Shared searcher handle
}

impl CodeSearch for TantivyIndexProvider {
    async fn search(
        &self,
        query: &str,
        language: Option<&str>,
        repo_url: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CodeSearchDoc>> {
        // 1. BM25 search on code_text field
        // 2. Filter by language/repo if specified
        // 3. Return top-K CodeSearchDoc results
    }
}
```

### Code Search Params (`CodeSearchParams`)

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `min_embedding_score` | 0.5 | Minimum embedding cosine similarity |
| `min_bm25_score` | -1.0 | BM25 threshold (negative = no limit) |
| `num_to_return` | 10 | Top-K results |
| `num_to_score` | 100 | Candidate pool size |

---

## 10.8 Index Storage Layout

### File System Structure

```
~/.tabby/
├── index/
│   ├── code/
│   │   └── tantivy/        # Tantivy segment files
│   │       ├── meta.json
│   │       ├── segments_v1/
│   │       └── ...
│   └── docs/
│       └── tantivy/        # Structured doc index (separate)
├── repos/
│   ├── https_github.com_org_repo1/    # Cloned git repos
│   └── https_github.com_org_repo2/
└── db/
    └── default.sqlite                  # EE SQLite (tracks indexing progress)
```

### Index Size Estimation

| Repo Size | Approximate Index Size |
|-----------|----------------------|
| 10K SLoC | ~5 MB |
| 100K SLoC | ~50 MB |
| 1M SLoC | ~500 MB |
| 10M+ SLoC | ~5 GB |
