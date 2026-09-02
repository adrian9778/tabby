# 第五章：API 参考

## 5.1 REST API Overview

### Base URL

```
http://localhost:8080         # Default (no TLS)
https://your-domain.com       # Production (recommended with reverse proxy)
```

### Authentication

| Header | 说明 |
|--------|------|
| `Authorization: Bearer <token>` | 用户身份认证（EE GraphQL + REST） |
| Registration token | 初始注册用，无需 JWT |

### Response Format

所有 JSON API 统一返回：

```json
{ "data": <response_body> }           // Success
{ "error": { "code": "<code>", "message": "<detail>" } }  // Error
```

---

## 5.2 `/v1/completions` — Code Completion

### Endpoint

```
POST /v1/completions
Content-Type: application/json
```

### Request Body

```json
{
  "language": "python",              // Source file language (e.g., "python", "rust", "typescript")
  "segments": {
    "prefix": "def fibonacci(n):\n    ",   // Code before cursor
    "suffix": "",                       // Code after cursor (FIM mode)
    "cursor_position": 27               // Cursor byte offset
  },
  "user": "user-123",                  // Optional: user identifier for analytics [EE]

  // Additional options
  "max_tokens": 128,                   // Override max_decoding_tokens
  "temperature": 0.2,                  // Model sampling temperature
  "stop": ["\n"]                       // Stop sequences
}
```

### Response Body (streaming)

```json
{
  "candidate": {
    "text": "\n    if n <= 1:\n        return n",
    "finish_reason": "length"            // "length" | "stop" | "error"
  },
  "id": "cmpl-abc123",
  "model": "tabbyml/StarCoder-1B"
}
```

### Prompt Template Variables

Tabby 在服务端构建 prompt，使用的变量（以 `<PRE>{prefix}<SUF>{suffix}<MID>` 为例）：

| 变量 | 来源 | 示例 |
|------|------|------|
| `{prefix}` | `segments.prefix` + LSP context + repo context | `def fibonacci(n):\n    if n <= 1:\n        return n\n    return ` |
| `{suffix}` | `segments.suffix` | `\n\nclass Test:\n    pass` |
| `{language}` | Request `language` field | `"python"` |

### FIM (Fill-in-the-Middle) 推理模型

支持 FIM 的模型需要在 `tabby.json` 或 config 中声明 `prompt_template`：

```json
// tabby.json (model directory)
{
  "prompt_template": "<PRE>{prefix}<SUF>{suffix}<MID>",
  "chat_template": "<s>{% for message in messages %}..."
}
```

常用模型的 prompt_template 对照表：

| 模型 | prompt_template |
|------|----------------|
| StarCoder-2 | `<fim_prefix>{prefix}<fim_suffix>{suffix}<fim_middle>` |
| CodeLlama | `<PRE>{prefix}<SUF>{suffix}<MID>` |
| Qwen-Coder | `<|fim_pad|>{prefix}<|eof_pad|><|pad_id|>{suffix}` |
| Codestral | `[PREFIX]{prefix}[SUFFIX]{suffix}[SUFFIX][MIDDLE]` |

---

## 5.3 `/v1/chat/completions` — Chat (Answer Engine)

### Endpoint

```
POST /v1/chat/completions
Content-Type: application/json
Accept: text/event-stream      // SSE streaming
```

### Request Body

```json
{
  "model": "tabbyml/Qwen2-1.5B-Instruct",
  "messages": [
    {
      "role": "user",
      "content": "如何在这个项目中添加一个新的 API endpoint？"
    }
  ],
  // Retrieval context options
  "search_query": "api endpoint pattern",   // Optional: custom search query
  "code_search_params": {
    "num_to_return": 10,
    "min_embedding_score": 0.5,
    "min_rrf_score": -1.0
  },

  // Generation options
  "temperature": 0.7,
  "max_tokens": 1024,
  "stream": true                           // Default: true (SSE)
}
```

### Response (SSE stream)

```
event: message
data: {"id":"chatcmpl-1","choices":[{"delta":{"content":"Tabby"}}]}

event: message
data: {"id":"chatcmpl-1","choices":[{"delta":{"content":"使用"}}]}

event: message
data: {"id":"chatcmpl-1","choices":[{"delta":{"content":"Axum"}}]}

event: done
data: {"finish_reason":"stop"}
```

### Chat 请求处理流程 (服务端)

```
Chat Request → Retrieval Service → Build system prompt with context
    │
    ├── Step 1: Tantivy code search (tree-sitter embeddings + BM25)
    ├── Step 2: Document search (web pages, custom docs)
    ├── Step 3: Repository context (recent files, declarations)
    └── Step 4: Forward full prompt to chat model
```

---

## 5.4 `/v1/events` — Usage Events

### Endpoint

```
POST /v1/events
Content-Type: application/json
```

### Request Body

```json
{
  "type": "view",                    // "view" | "select" | "completion_time"
  "completion_id": "cmpl-abc123",    // From completion response
  "event_id": "evt-def456",          // Unique event ID (client-generated)
  "model": "tabbyml/StarCoder-1B",
  "language": "python",
  "file_path": "/path/to/file.py",
  "timestamp": 1710000000,
  "metadata": {
    "elapsed_ms": 150,               // Time to first token
    "choices_count": 3               // Number of suggestions shown
  }
}
```

### Event Types

| type | 触发时机 | 用途 |
|------|---------|------|
| `view` | 补全建议显示给用户时 | Count impression |
| `select` | 用户接受补全内容 | Count acceptance rate |
| `completion_time` | 从请求到首 token 的延迟 | Latency analytics |

---

## 5.5 `/v1/health` — Model Health Check

### Endpoint

```
GET /v1/health
```

### Response Body

```json
{
  "state": "ready",                    // "loading" | "ready" | "error"
  "model": "tabbyml/StarCoder-1B",     // Completion model name
  "chat_model": "tabbyml/Qwen2-1.5B-Instruct",  // Chat model name (nullable)
  "embedding_model": "tabbyml/Nomic-Embed-Text",
  "arch": "cuda",                      // "cuda" | "rocm" | "metal" | "cpu"
  "device": "cuda:0",                  // GPU device string
  "parallelism": 4,                    // GPU layer parallelism
  "fp16": true,                        // FP16 support flag
  "rtn_quantized": false,              // Row-quantization status
  "fast_attention": false,             // Fast attention mode
  "context_length": 4096,              // Model context length
  "error": null                        // Error message if state == "error"
}
```

### States

| state | 含义 |
|-------|------|
| `loading` | 模型正在加载/下载中，不可用 |
| `ready` | 模型已就绪，可以处理请求 |
| `error` | 模型加载失败，检查 `error` 字段 |

---

## 5.6 `/v1/models` — Model Listing

### Endpoint

```
GET /v1/models
```

### Response Body

```json
{
  "data": [
    {
      "id": "tabbyml/StarCoder-1B",
      "name": "StarCoder-1B",
      "type": "completion"
    },
    {
      "id": "tabbyml/Qwen2-1.5B-Instruct",
      "name": "Qwen2-1.5B-Instruct",
      "type": "chat"
    }
  ]
}
```

---

## 5.7 `/v1/server/settings` — Server Settings

### Endpoint

```
GET /v1/server/settings
```

### Response Body (Open Source)

```json
{
  "data": {
    "allow_anonymous_tokens": true,
    "custom_docs": [],
    "link_account_enabled": true
  }
}
```

---

## 5.8 Metrics Endpoint — `/metrics`

### Endpoint

```
GET /metrics
Content-Type: text/plain; charset=utf-8
```

### Prometheus Format (示例)

```prometheus
# HELP tabby_completion_total Total number of completion requests
# TYPE tabby_completion_total counter
tabby_completion_total{model="tabbyml/StarCoder-1B"} 15234

# HELP tabby_chat_total Total number of chat requests
# TYPE tabby_chat_total counter
tabby_chat_total{model="tabbyml/Qwen2-1.5B-Instruct"} 8921

# HELP tabby_completion_latency_seconds Latency histogram
# TYPE tabby_completion_latency_seconds histogram
tabby_completion_latency_seconds_bucket{le="0.1"} 3000
tabby_completion_latency_seconds_bucket{le="0.5"} 12000
tabby_completion_latency_seconds_bucket{le="1.0"} 14800
tabby_completion_latency_seconds_bucket{le="+Inf"} 15234
tabby_completion_latency_seconds_sum 6789.5
tabby_completion_latency_seconds_count 15234

# HELP tabby_event_accept_total Completion acceptance count
# TYPE tabby_event_accept_total counter
tabby_event_accept_total{model="tabbyml/StarCoder-1B"} 4521

# HELP tabby_index_commit_size_bytes Index size in bytes
# TYPE tabby_index_commit_size_bytes gauge
tabby_index_commit_size_bytes 89234567
```

### Key Metrics

| Metric | Type | Labels | 含义 |
|--------|------|--------|------|
| `tabby_completion_total` | counter | model | 补全请求总数 |
| `tabby_chat_total` | counter | model | 聊天请求总数 |
| `tabby_completion_latency_seconds` | histogram | model | 延迟分布 |
| `tabby_event_accept_total` | counter | model | 用户接受数 |
| `tabby_index_commit_size_bytes` | gauge | (none) | 索引大小 |
| `tabby_db_connections_active` | gauge | (none) | DB 连接池活跃数 [EE] |

---

## 5.9 GraphQL API (Enterprise Edition)

### Endpoint

```
POST /graphql
Content-Type: application/json
Authorization: Bearer <jwt_token>
```

### Schema Introspection

```
GET /graphql/schema    # Returns the full GraphQL SDL
```

### Query Example — Users List

```graphql
query {
  users(first: 20, after: null) {
    edges {
      node {
        id
        name
        email
        role
        active
        createdAt
      }
      cursor
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
```

### Query Example — Usage Stats

```graphql
query {
  dailyStats(
    user: "user-uuid",
    since: "2025-01-01",
    until: "2025-01-31"
  ) {
    completion { view select }
    chat { view select }
  }
}
```

### Mutation Example — Register User

```graphql
mutation {
  register(
    name: "Alice",
    email: "alice@example.com",
    password: "secure-password"
  ) {
    id
    name
    email
    role
  }
}
```

### Subscription Example — Real-time Chat

```graphql
subscription {
  createThreadRun(threadId: "thread-uuid", message: "What does this function do?") {
    delta      // Text chunk (streaming)
    done       // true when stream complete
  }
}
```

---

## 5.10 OpenAPI Spec & SDKs

### OpenAPI JSON

项目内置 OpenAPI spec 生成，可通过以下位置获取：

| 位置 | 说明 |
|------|------|
| `clients/tabby-openapi/openapi.json` | 仓库中的最新 spec |
| `/docs` (运行时) | Server 启动后可访问的 Swagger UI |
| `crates/tabby/src/serve.rs` | 运行时通过 `utoipa` + `axum-extra` 自动合并 |

### SDK 生成

```bash
# Using OpenAPI Generator
npx @openapitools/openapi-generator-cli generate \
  -i https://your-tabby-server/docs/json \
  -g typescript-axios \
  -o ./tabby-sdk

# Or use the pre-built tabby-openapi npm package
npm install @tabbyml/tabby-openapi
```

---

## 5.11 Error Codes Reference

| HTTP Status | 含义 | 常见原因 |
|------------|------|---------|
| `200 OK` | Success | 正常响应 |
| `400 Bad Request` | 请求格式错误 | 缺少必填字段、JSON 解析失败 |
| `401 Unauthorized` | 未认证 | JWT 过期/无效；REST API 需要 token |
| `403 Forbidden` | 权限不足 [EE] | RBAC Access Policy deny |
| `404 Not Found` | 资源不存在 | 无效的 model ID / repo ID |
| `429 Too Many Requests` | Rate limited | 超出 rate_limit (1200 req/min) |
| `500 Internal Server Error` | Server error | 模型加载失败、数据库错误 |
| `503 Service Unavailable` | 服务不可用 | 模型加载中 (`state: "loading"`) |

### Error Body Format

```json
{
  "error": {
    "code": "MODEL_LOADING",
    "message": "Model is still loading, please try again later",
    "retry_after": 30
  }
}
```
