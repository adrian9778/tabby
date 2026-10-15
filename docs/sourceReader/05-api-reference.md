# 第五章：API 参考

> 本章所有端点、字段名、默认值均来自当前提交实测。路由注册在 [crates/tabby/src/serve.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby/src/serve.rs) · `api_router` `+0`；EE 侧追加路由在 [ee/tabby-webserver/src/routes/mod.rs](file:///Users/vincent/workspace/16_AI/tabby/ee/tabby-webserver/src/routes/mod.rs) · `create` `+0`。
> **注意**：本章刻意不沿用旧版「统一 `{data}` / `{error}` 信封」的说法 —— 代码里不存在这样的响应包装，见 §5.1.3。

## 5.1 REST API Overview

### 5.1.1 Base URL 与路由前缀

```
http://localhost:8080         # 默认 host 0.0.0.0 / port 8080（--host / --port）
https://your-domain.com       # 生产建议反代 + TLS
```

| 前缀 | 定位 | 说明 |
|------|------|------|
| `/v1/*` | 稳定 API | completions / chat / events / health |
| `/v1beta/*` | 前向兼容 / 实验中 | models、server_setting、chat 别名、ingestion |
| `/graphql`、`/subscriptions` | EE | 见 §5.9 |
| `/metrics` | 观测 | 见 §5.8 |
| `/swagger-ui`、`/api-docs/openapi.json` | 文档 | 见 §5.10 |

> 实测路径清单（`api_router` 内逐条 `.route()`）：`/v1/events`、`/v1/health`（GET + POST 都注册）、`/v1beta/models`、`/v1/completions`、`/v1/chat/completions`、`/v1beta/chat/completions`、`/v1beta/server_setting`。
> **`/v1/models` 不存在**（旧版文档常见误写），模型列表是 `/v1beta/models`。

### 5.1.2 Authentication

认证行为取决于是否启用 EE webserver，这是文档里最容易写错的一点：

| 运行形态 | `/v1/*` 是否需要 token |
|---------|----------------------|
| 纯开源 core（无 `ee` feature，或 `serve --no-webserver`） | **不需要**，任何请求直通 |
| EE webserver 已启用 | **需要** `Authorization: Bearer <token>` |

- 追加请求头 `x-tabby-user: <user_id>`（常量在 [crates/tabby-common/src/constants.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby-common/src/constants.rs) · `USER_HEADER_FIELD_NAME` `+0`；解码器 [crates/tabby-common/src/axum.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby-common/src/axum.rs) · `struct MaybeUser` `+0`）。注意值是**用户 ID**，不是邮箱。
- 注入 `AllowedCodeRepository` extension（`crates/tabby-common/src/axum.rs` · `AllowedCodeRepository` `+0`），供检索侧限定可访问的 git 仓库。

**限流**：`user_id` 存在时先过 [ee/tabby-webserver/src/rate_limit.rs](file:///Users/vincent/workspace/16_AI/tabby/ee/tabby-webserver/src/rate_limit.rs) · `UserRateLimiter::is_allowed` `+0`，超限直接 **429**（空 body）。

| 参数 | 实测值 |
|------|--------|
| 速率 | 120 req/min（`static USER_REQUEST_LIMIT_PER_MINUTE` `+0`） |
| 桶容量 | 240（`max_tokens = limit * 2`） |
| 初始可用 | 120 |
| 每用户独立 limiter | 是，缓存在 `TimedCache`，lifespan 60s |
| 豁免路径 | 路径以 `/v1/health` 或 `/v1beta/health` 结尾 |

> 别把这个 120 和第十二章 `[http] rate_limit = 1200` 混为一谈：后者是**出站**限流（Tabby 作为客户端调用外部 HTTP provider），前者是**入站**按用户限流，只在 EE 生效。

### 5.1.3 Response Format —— 没有统一信封

实测结论：**不存在** `{ "data": ... }` / `{ "error": { code, message } }` 这类包装。

| 情形 | 实际响应 |
|------|---------|
| 成功 | 直接是该端点的资源对象 JSON（如 `CompletionResponse`），不套 `data` |
| `/v1/completions` 失败 | `StatusCode::BAD_REQUEST`（400），**body 为空**；错误文本只进 `warn!` 日志 |
| `/v1/chat/completions` 失败 | 500，body 为空 |
| `/v1/events` 失败 | 400，body 为空 |
| 认证失败 | 401，body 为空 |
| 限流 | 429，body 为空 |
| JSON 反序列化失败 / 缺 `Authorization: Bearer` 头 | axum 默认 400 / 415，纯文本 |

`crates/tabby/src/routes/completions.rs` · `completions` `+0` 里那条 `Err(err) => { warn!("{}", err); Err(StatusCode::BAD_REQUEST) }` 是**唯一**的错误出口 —— 也就是说客户端拿不到机器可读的错误码，排障要看服务端日志。

---

## 5.2 `/v1/completions` — Code Completion

### Endpoint

```
POST /v1/completions
Content-Type: application/json
```

处理函数 [crates/tabby/src/routes/completions.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby/src/routes/completions.rs) · `completions` `+0`；该路由额外包了一层 `TimeoutLayer`，超时秒数取 `[server] completion_timeout`（默认 30）。
若服务端**没有**配置 completion 模型，路由仍注册，但 handler 是 `StatusCode::NOT_IMPLEMENTED` ⇒ 恒返回 **501**。

### Request Body

真实类型 [crates/tabby/src/services/completion.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby/src/services/completion.rs) · `CompletionRequest` `+0`，字段就这 7 个：

```jsonc
{
  "language": "python",
  "segments": {
    "prefix": "def fibonacci(n):\n    ",
    "suffix": "\n    return fibonacci(n-1) + fibonacci(n-2)",
    "filepath": "fib.py",                                    // 示例文件名，仓库中不存在
    "git_url": "https://github.com/org/repo",
    "declarations": [{ "filepath": "utils.py", "body": "def gcd(a, b): ..." }],   // 示例文件名，仓库中不存在
    "relevant_snippets_from_changed_files": [{ "filepath": "a.py", "body": "...", "score": 0.83 }],  // 示例文件名，仓库中不存在
    "relevant_snippets_from_recently_opened_files": [],
    "clipboard": "...",
    "edit_history": {
      "original_code": "...",
      "edits_diff": "...",
      "current_version": "..."
    }
  },
  "user": "user-uuid",
  "debug_options": {
    "raw_prompt": "...",
    "return_snippets": false,
    "return_prompt": false,
    "disable_retrieval_augmented_code_completion": false
  },
  "temperature": 0.2,
  "seed": 42,
  "mode": "standard"
}
```

> 上面这段是**请求结构示例**，其中标注了「示例文件名」的 `filepath` 值都不是仓库里真实存在的文件；`filepath` 的真实语义见下表。

| 字段 | 类型 | 说明 |
|------|------|------|
| `language` | `Option<String>` | VSCode 语言标识；缺省按 `"unknown"` 处理 |
| `segments` | `Option<Segments>` | 设置了它，推理时忽略 `debug_options.raw_prompt` 以外的裸 prompt |
| `user` | `Option<String>` | 匿名分析标识；**EE 下会被 `x-tabby-user` 头覆盖** |
| `debug_options.raw_prompt` | `Option<String>` | 直接透传给推理引擎，跳过服务端 prompt 组装 |
| `debug_options.return_snippets` / `return_prompt` | `bool`（默认 false） | 控制响应里是否回填 `debug_data` |
| `debug_options.disable_retrieval_augmented_code_completion` | `bool`（默认 false） | 关掉 RAG 检索 |
| `temperature` | `Option<f32>` | 透传给 `CodeGenerationOptionsBuilder::sampling_temperature` |
| `seed` | `Option<u64>` | 透传给 `builder.seed` |
| `mode` | `String` | `"standard"`（默认）或 `"next_edit_suggestion"` |

> **纠正旧文档三处**：①请求体里**没有** `max_tokens`、**没有** `stop` —— 生成长度由服务端 `[completion] max_decoding_tokens`（默认 64）决定，停止序列由 prompt 模板/后处理规则决定，客户端不可覆盖；②`segments` 里**没有** `cursor_position`；③顶层**没有** `prompt` 字段，裸 prompt 走 `debug_options.raw_prompt`。

`Segments` 定义在同文件 · `Segments` `+0`，10 个字段全为 `Option` 除 `prefix`。其中 `edit_history`（`EditHistory` `+0`）**仅当 `mode = "next_edit_suggestion"` 时必填**。

### Response Body

真实类型 · `CompletionResponse` `+0`：

```jsonc
{
  "id": "cmpl-0b9c0c2e-1f4d-4d8e-9a3b-7c2d1e0f5a6b",
  "choices": [
    { "index": 0, "text": "\n    if n <= 1:\n        return n" }
  ],
  "debug_data": {
    "snippets": [{ "filepath": "utils.py", "body": "...", "score": 0.83 }],   // 示例文件名，仓库中不存在
    "prompt": "def fibonacci(n):\n    ..."
  },
  "mode": "standard"
}
```

| 字段 | 事实 |
|------|------|
| `id` | 服务端生成，格式 `cmpl-{uuid}`（见 `completion.rs` · `CompletionService::generate` 函数体里的 `format!("cmpl-{}", uuid::Uuid::new_v4())`） |
| `choices` | **当前恒为 1 个元素**，`Choice::new` 硬编码 `index: 0` |
| `choices[].text` | 生成内容，已过后处理规则（见第七章 / `tabby-inference` 的 postprocess） |
| `debug_data` | `Option`，两个子字段都是 `skip_serializing_if = Option::is_none`；只在对应 `debug_options.return_*` 为 true 时出现 |
| `mode` | `#[serde(default = "default_standard_mode")]` 回填 |

> **纠正**：旧文档写的 `{ "candidate": { "text", "finish_reason" }, "model" }` 是 **Tabby 0.1x 时代**的响应形态，当前代码里没有 `candidate`、没有 `finish_reason`、没有顶层 `model`。想要 `finish_reason` 得走 `/v1/chat/completions`。

### Prompt Template Variables

服务端组装 prompt 的实现是 [crates/tabby/src/services/completion/completion_prompt.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby/src/services/completion/completion_prompt.rs) · `build_prompt` `+0`，只有一行模板展开：

```rust
strfmt!(prompt_template, prefix => prefix, suffix => suffix).unwrap()
```

| 变量 | 来源 |
|------|------|
| `{prefix}` | 由 `segments.prefix` + LSP declarations + 检索 snippets + 最近编辑文件 snippets 拼装后的结果 |
| `{suffix}` | `segments.suffix` |

> **只有这两个变量**。**没有** `{language}` 占位符 —— 语言只用于选择 snippets 检索策略和后处理规则，不进模板。若 `prompt_template` 为 `None`，`build_prompt` 直接返回裸 `prefix`（FIM 由模型自身完成）。

### FIM 模型声明

本地 GGUF 模型的元数据文件是模型目录下的 `tabby.json`。**写入方**是 [crates/tabby-common/src/registry.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby-common/src/registry.rs) · `save_model_info` `+0`（内容来自 `registry-tabby` 注册表的 `ModelInfo`）；**读取方**是 [crates/llama-cpp-server/src/lib.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/llama-cpp-server/src/lib.rs) · `PromptInfo` `+0` + `resolve_prompt_info` `+0`：

```json
// ~/.tabby/models/<ModelId>/tabby.json  —— 只有这两个字段
{
  "prompt_template": "<PRE>{prefix}<SUF>{suffix}<MID>",
  "chat_template": "..."
}
```

常用模型的 completion prompt 模板形态（**以模型仓库自带的 `tabby.json` 为准，此处仅示意结构**）：

| 模型系列 | 模板形态（示意） |
|---------|-----------------|
| StarCoder-2 | `<fim_prefix>{prefix}<fim_suffix>{suffix}<fim_middle>` |
| CodeLlama | `<PRE>{prefix}<SUF>{suffix}<MID>` |
| Qwen-Coder | `<\|fim_prefix\|>{prefix}<\|fim_middle\|>{suffix}` |
| Codestral | `[PREFIX]{prefix}[SUFFIX]{suffix}[MIDDLE]` |

外部 HTTP provider 走配置项 `[model.completion.http] prompt_template`（第十二章），约束同样是「必须同时含 `{prefix}` 与 `{suffix}`」。

## 5.3 `/v1/chat/completions` — Chat（OpenAI 兼容）

### Endpoint

```
POST /v1/chat/completions
POST /v1beta/chat/completions        # 同一 handler 的别名路由，前向兼容用
Content-Type: application/json
```

处理函数 [crates/tabby/src/routes/chat.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby/src/routes/chat.rs) · `chat_completions` `+0`，状态类型 `ChatState` `+0`（持有 `Arc<dyn ChatCompletionStream>` + `Arc<dyn EventLogger>`）。

### Request Body

请求体类型**不是 tabby 自定义的**，而是直接复用 `async-openai-alt = 0.26.2` 的 `CreateChatCompletionRequest` —— 即标准 OpenAI Chat Completions 载荷（`model`、`messages`、`temperature`、`top_p`、`max_tokens`、`stop`、`presence_penalty`、`frequency_penalty`、`seed`、`user`、`stream` 等）。

```json
{
  "model": "Qwen2-1.5B-Instruct",
  "messages": [
    { "role": "user", "content": "如何在这个项目中添加一个新的 API endpoint？" }
  ],
  "temperature": 0.7,
  "max_tokens": 1024,
  "user": "user-uuid"
}
```

三个实测行为需要写进契约：

| 行为 | 事实 |
|------|------|
| `stream` 字段 | **被忽略**。handler 无条件返回 `Sse`，即使 `stream: false` 也是 SSE 流 |
| `model` 字段 | 由具体 provider binding 决定是否使用；本地模型场景下服务端已固定模型，值仅透传 |
| `user` 字段 | EE 下若中间件已注入 `x-tabby-user`，则 `request.user.replace(user)` **覆盖客户端传值** |

> **纠正**：旧文档里的 `search_query` 和 `code_search_params: { num_to_return, min_embedding_score, min_rrf_score }` **不在这个 REST 请求体里**。REST `/v1/chat/completions` 是「裸转发 + 流式」，检索发生在 Answer Service 内部；能被外部调参的检索选项只存在于 GraphQL 侧（见 §5.9 的 `ThreadRunOptionsInput` / `CodeSearchParamsOverrideInput`）。
> 另外 `min_rrf_score` 的真实类型是 `f64`（`Option<f64>`），不是文档里的 `-1.0` 这类带符号约定；`CodeSearchParams` 的五个可调项是 `min_embedding_score` / `min_bm25_score` / `min_rrf_score` / `num_to_return` / `num_to_score`。

### Response（SSE）

帧格式由 axum 的 `Event::default().json_data(chunk)` 生成 —— **只有 `data:` 行，没有 `event:` 名**，每帧负载是一个 `ChatCompletionChunk`：

```
data: {"id":"chatcmpl-...","object":"chat.completion.chunk","created":1710000000,"model":"Qwen2-1.5B-Instruct","choices":[{"index":0,"delta":{"content":"Tabby"},"finish_reason":null}]}

data: {"id":"chatcmpl-...","choices":[{"index":0,"delta":{"content":"使用"},"finish_reason":"stop"}]}

: keep-alive
```

| 细节 | 事实 |
|------|------|
| 事件名 | 无。客户端**不能**按 `event: message` 分派（旧文档写的 `event: message` / `event: done` 都不存在） |
| 结束标志 | 流自然关闭。底层 binding 抛 `OpenAIError::StreamError("Stream ended")` 时 handler `break`，该哨兵**不会**发给客户端 |
| keep-alive | `Sse::new(s).keep_alive(KeepAlive::default())`，输出 `:` 注释行 |
| 中间出错 | `yield Err(...)` ⇒ SSE 流中断，无结构化错误帧 |
| 埋点 | 请求进入即 `logger.log(user, Event::ChatCompletion {})`（**在流开始前**，且不记录内容） |

### 状态码

| 状态 | 触发 |
|------|------|
| 200 | 正常（含流式） |
| 401 | EE 下 token 缺失/无效 |
| 415 / 400 | 缺 `Content-Type: application/json` / body 反序列化失败（axum 默认） |
| 405 | **仅在 `#[utoipa::path]` 注解里声明**（「未配置 chat model 时返回 405」），当前 handler 代码路径没有实现它 |
| 422 | 同上 —— 注解声明，handler 未实现 |
| 501 | 服务端未配置 chat model 时，`api_router` 注册的兜底 handler 是 `StatusCode::NOT_IMPLEMENTED` |
| 500 | `chat_stream()` 返回 Err（`chat_completions` 里唯一的显式错误分支） |

> 这是一处「文档契约 ≠ 实现」的真实缺口：**405/422 只存在于 OpenAPI 注解**。写集成测试时不要依赖它们；未配置 chat model 的实际表现是 501。

### Chat 请求处理流程（服务端）

```
REST /v1/chat/completions            GraphQL createThreadRun（Answer Engine）
        │                                     │
        │ chat_stream(request)                │ ① CodeQueryInput → retrieval()
        │                                     │ ② DocQueryInput  → web/document 检索
        │                                     │ ③ 组装 system prompt + few-shot
        ▼                                     ▼
   ChatCompletionStream 实现（本地 llama.cpp / Ollama / OpenAI 兼容 / Mistral / Azure / …）
        │
        ▼
   SSE: data: ChatCompletionChunk …（流关闭即结束）
```

REST 路径本身**不做 RAG**；RAG 只发生在 `ee/tabby-webserver/src/service/answer.rs` 那条 GraphQL 链路里。这一点是旧文档「Step 1 Tantivy code search … Step 4 Forward full prompt」被误挂到 REST 端点上的根源。

---

## 5.4 `/v1/events` — Usage Events

### Endpoint

```
POST /v1/events?select_kind=line     # select_kind 是 QUERY 参数，不在 body 里
Content-Type: application/json
```

处理函数 [crates/tabby/src/routes/events.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby/src/routes/events.rs) · `log_event` `+0`。

### Request Body

真实类型 [crates/tabby-common/src/api/event.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby-common/src/api/event.rs) · `LogEventRequest` `+0`，**只有 5 个字段**：

```json
{
  "type": "select",
  "completion_id": "cmpl-0b9c0c2e-1f4d-4d8e-9a3b-7c2d1e0f5a6b",
  "choice_index": 0,
  "view_id": "可选，客户端生成的视图标识",
  "elapsed": 150
}
```

| 字段 | 类型 | 序列化名 |
|------|------|---------|
| `event_type: String` | 必填 | 通过 `#[serde(rename = "type")]` 对外是 **`type`** |
| `completion_id` | `String` | 必填 |
| `choice_index` | `u32` | 必填 |
| `view_id` | `Option<String>` | 可空 |
| `elapsed` | `Option<u32>` | 可空，单位 ms |

### Event Types

`type` 的合法值**只有三个**（handler 用 `if / else if` 硬比较，其余一律 400）：

| type | 落库事件（`enum Event` `+0`） | 附加来源 |
|------|------------------------------|---------|
| `view` | `Event::View { completion_id, choice_index, view_id }` | — |
| `select` | `Event::Select { completion_id, choice_index, kind, view_id, elapsed }` | `kind` 由 query 参数 `select_kind=line` 决定，否则为 `None` |
| `dismiss` | `Event::Dismiss { completion_id, choice_index, view_id, elapsed }` | — |

> **纠正三处**：①没有 `completion_time` 这个 type，延迟信息靠 `select`/`dismiss` 携带的 `elapsed`；②没有 `model` / `language` / `file_path` / `timestamp` / `metadata` 字段 —— 时间戳由 `EventLogger::log` 内部用 `SystemTime` 生成毫秒 `ts`（同文件 `timestamp` `+0` + `LogEntry` `+0`），语言与文件路径在服务端补全时已随 `Event::Completion` 记录；③`select_kind` 在 **URL query** 上而不是 body 里，客户端很容易传错位置然后静默丢掉 `kind`。

### 另两个不发 HTTP 的事件

`enum Event` 还有 `Completion { completion_id, language, prompt, segments, choices, user_agent, … }` 与 `ChatCompletion {}` 两个变体，由**服务端内部**记录（补全生成时 / chat 请求进入时），不通过 `/v1/events` 提交。`Event::Completion` 里包含完整 prompt —— 这是审计与 eval 复现的数据源，注意其敏感性。

### Response

只有状态码：200（三类合法事件）/ 400（未知 `type`）/ EE 下 401、429。**没有响应体**。

## 5.5 `/v1/health` — Model Health Check

### Endpoint

```
GET  /v1/health
POST /v1/health        # 同一 handler，两个 method 都注册了（兼容旧客户端）
```

处理函数 [crates/tabby/src/routes/health.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby/src/routes/health.rs) · `health` `+0`；它返回的是 `api_router` 启动时构造好的**快照**，不是每次请求重新探测 —— 也就是说这个端点**永远不会**反映「模型此刻卡住了」，只反映配置与构建信息。

### Response Body

真实类型 [crates/tabby/src/services/health.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby/src/services/health.rs) · `HealthState` `+0`：

```json
{
  "model": "StarCoder-1B",
  "chat_model": "Qwen2-1.5B-Instruct",
  "chat_device": "cuda",
  "device": "cuda",
  "cuda_devices": ["NVIDIA GeForce RTX 3060"],
  "models": {
    "completion": { "local": { "model_id": "StarCoder-1B", "device": "cuda", "cuda_devices": ["NVIDIA GeForce RTX 3060"] } },
    "chat":       { "remote": { "kind": "openai/chat", "model_name": "gpt-4o-mini", "api_endpoint": "https://api.openai.com/v1" } },
    "embedding":  { "local": { "model_id": "tabbyml/TabbyML-Embedding-Code-v1", "device": "cuda", "cuda_devices": [] } }
  },
  "arch": "aarch64",
  "cpu_info": "Apple M1 Pro",
  "cpu_count": 10,
  "version": {
    "build_date": "2026-08-12",
    "build_timestamp": "2026-08-12T03:41:07.123456789Z",
    "git_sha": "f05db40e...",
    "git_describe": "v0.33.0-dev.0"
  },
  "webserver": true
}
```

| 字段 | 类型 / 来源 |
|------|------------|
| `model`, `chat_model`, `chat_device` | `Option<String>`，`skip_serializing_if`；`model`/`chat_model` 由 `to_model_name` `+0` 得出 —— HTTP 模型取 `model_name`，取不到则**填字符串 `"Remote"`**，本地模型取 `model_id`。**源码注释标明这三个「slated for future deprecation」**，新客户端请读 `models` |
| `device` | `--device` 的 strum 序列化值：`cpu` \| `cuda` \| `rocm` \| `metal` \| `vulkan`（`enum Device` 在 [crates/tabby/src/main.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby/src/main.rs) · `enum Device` `+0`） |
| `cuda_devices` | 用 NVML 枚举的 GPU 名；macOS / 未挂 GPU 的容器里 `Nvml::init()` 失败 ⇒ **空数组**（`read_cuda_devices` `+0` 的注释明确说明了这一点） |
| `models` | `ModelsHealth` `+0`：`completion`/`chat` 为 `Option`，`embedding` **必填** |
| `models.*` 形态 | `enum ModelHealth` `+0` 是 tagged enum ⇒ JSON 里是 `{"local": {...}}` 或 `{"remote": {...}}` 二选一 |
| `remote` 子对象 | `RemoteModelHealth` `+0`：`kind`（必填）、`model_name`（可空）、`api_endpoint`（必填，缺省为**空字符串**不是 null） |
| `local` 子对象 | `LocalModelHealth` `+0`：`model_id`、`device`、`cuda_devices`（空则整个字段不出现） |
| `arch` | `std::env::consts::ARCH`，即 **`aarch64` / `x86_64`** 这类 CPU 架构 |
| `cpu_info` / `cpu_count` | `sysinfo` 实测，取第一块 CPU 的 brand；拿不到则 `"unknown"` |
| `version` | `Version` `+0`，四个字段全部来自编译期 `VERGEN_*` 环境变量 |
| `webserver` | `Option<bool>`，EE 是否启用 |

> **纠正旧文档一整张表**：`state: "loading" | "ready" | "error"` **不存在**。当前 `HealthState` 里没有任何状态机字段，也没有 `error`、`fp16`、`rtn_quantized`、`fast_attention`、`context_length`、`parallelism`、`"arch": "cuda"`（`arch` 是 CPU 架构，GPU 后端在 `device`）这些键。
> 连带的后果：**「模型加载中返回 503」这条常见说法也是错的** —— 见 §5.11。

### 判活该怎么做

由于 `/v1/health` 是配置快照，可用的判活信号只有：

1. HTTP 层能否连通（进程起来了就 200）；
2. `models.completion` 是否存在 —— 为 `null`/缺失 ⇒ 服务端没配 completion 模型 ⇒ `/v1/completions` 恒 501；
3. `models.chat` 同理对应 `/v1/chat/completions`；
4. `version.git_describe` 用于确认线上跑的是哪个构建。

---

## 5.6 `/v1beta/models` — 支持的模型列表

### Endpoint

```
GET /v1beta/models
```

处理函数 [crates/tabby/src/routes/models.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby/src/routes/models.rs) · `models` `+0`，状态 `ModelInfo` `+0`（在 `api_router` 里由 `ModelInfo::from(config)` 一次性构造）。

### Response Body

```json
{
  "completion": ["TabbyML-StarCoder-1B", "Qwen2.5-Coder-7B"],
  "chat": ["Qwen2-7B-Instruct"]
}
```

真实语义（见 `impl From<Config> for ModelInfo` `+0`）：**只有当 completion / chat 配成 HTTP 模型、且该 `[model.*.http]` 段落里显式写了 `supported_models` 时才有值**，否则字段是 `null`（`Option<Vec<String>>`，无 `skip_serializing`，所以会序列化成 `"completion": null`）。

> **纠正**：旧文档写 `GET /v1/models` 返回 `[{id, name, type}]` 数组 —— 路径、字段名、结构三者都不对。这里返回的是一个**对象**，两个键都是**字符串数组**，不是对象数组，也没有 `type`  discriminator。
> 另外这个端点**没有**登记进 `ApiDoc` 的 `paths(...)` 列表，所以它**不出现在 `/api-docs/openapi.json` 里** —— 别拿 OpenAPI spec 当路由全集。

---

## 5.7 `/v1beta/server_setting` — 服务端开关

### Endpoint

```
GET /v1beta/server_setting
```

| 实现 | 位置 | 返回 |
|------|------|------|
| 开源 core | [crates/tabby/src/routes/server_setting.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby/src/routes/server_setting.rs) · `setting` `+0` | 硬编码 `{ "disable_client_side_telemetry": false }` |
| EE 覆盖 | [ee/tabby-webserver/src/routes/mod.rs](file:///Users/vincent/workspace/16_AI/tabby/ee/tabby-webserver/src/routes/mod.rs) · `server_setting` `+0`（在 `create` `+0` 里用同名 route 覆盖 core） | 读 `setting().read_security_setting()` ⇒ 真实配置值 |

类型只有一个字段：[crates/tabby-common/src/api/server_setting.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby-common/src/api/server_setting.rs) · `ServerSetting` `+0`。

```json
{ "disable_client_side_telemetry": true }
```

失败路径：EE 版读配置出错 ⇒ 500（`warn!` 记日志，body 空）。

> **纠正**：旧文档写的 `GET /v1/server/settings` 与 `{ allow_anonymous_tokens, custom_docs, link_account_enabled }` 全部不存在。另外 `EEApiDoc` 的 `components(schemas(...))` 里**没有**注册 `ServerSetting`，`Makefile` 的 `update-openapi-doc` 目标反而显式 `delpaths` 掉了 `["components","schemas","ServerSetting"]` 与 `["paths","/v1beta/server_setting"]` —— 说明这个端点是有意不对外承诺契约的。

---

## 5.8 Metrics Endpoint — `/metrics`

### Endpoint

```
GET /metrics
```

注册点在 [crates/tabby/src/routes/mod.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby/src/routes/mod.rs) · `run_app` `+0`：`api` router 上挂 `PrometheusMetricLayer::pair()`，再把 `prometheus_handle` 作为 state 绑到 `/metrics`；handler 是 `crates/tabby/src/routes/metrics.rs` · `metrics` `+0`，内容就是 `handle.render()`。

### 实际指标全集

**全仓没有任何自定义 `tabby_*` 指标**（已按 `register_counter` / `register_histogram` / `counter!` / `histogram!` 检索确认）。`/metrics` 暴露的全部是 `axum-prometheus 0.6.1` 默认层产生的四条：

| Metric | Type | Labels | 含义 |
|--------|------|--------|------|
| `axum_http_requests_total` | counter | `endpoint`, `method`, `status` | 请求总数 |
| `axum_http_requests_duration_seconds` | histogram | `endpoint`, `method`, `status` | 请求耗时分布 |
| `axum_http_requests_pending` | gauge | `endpoint`, `method` | 在途请求数 |
| `axum_http_response_body_size` | histogram | `endpoint`, `method` | 响应体大小 |

`endpoint` 取的是 **axum 路由 pattern**（如 `/v1/completions`），不是带具体 id 的路径，因此基数可控。

> **纠正**：旧文档表里的 `tabby_completion_total`、`tabby_chat_total`、`tabby_completion_latency_seconds`、`tabby_event_accept_total`、`tabby_index_commit_size_bytes`、`tabby_db_connections_active` **六个指标名一个都不存在**，示例 Prometheus 文本也是虚构的。
> 需要「接受率 / 视图数 / 语言维度」这类业务指标，唯一数据源是 §5.4 事件落到 `usage/` 目录后的 JSONL，或 EE 的分析接口（第十二章 / 第九章的 `dailyStats`）。`/metrics` 只有 HTTP 层指标。

### 用途建议

| 想观测 | 用哪个指标 |
|-------|-----------|
| 补全 P99 延迟 | `axum_http_requests_duration_seconds_bucket{endpoint="/v1/completions"}` |
| 5xx 比例 | `axum_http_requests_total{status=~"5.."}` |
| 并发压力 | `axum_http_requests_pending{endpoint="/v1/completions"}` |
| 是否被 429 限流 | `axum_http_requests_total{status="429"}` |

## 5.9 GraphQL API (Enterprise Edition)

路由全部注册在 [ee/tabby-webserver/src/routes/mod.rs](file:///Users/vincent/workspace/16_AI/tabby/ee/tabby-webserver/src/routes/mod.rs) · `create` `+0`，schema 由 `ee/tabby-schema/src/schema/mod.rs` · `create_schema` `+0` 构造（juniper `RootNode<Query, Mutation, Subscription>`）。

### Endpoints

| 方法 + 路径 | handler | 用途 |
|-----------|---------|------|
| `POST /graphql` | `ee/tabby-webserver/src/axum/mod.rs` · `graphql` `+0` | 查询 / mutation |
| `GET /subscriptions` | `ee/tabby-webserver/src/axum/mod.rs` · `subscriptions` `+0` | WebSocket 订阅 |
| `GET /graphql` | `juniper_axum::playground("/graphql", "/subscriptions")` | 浏览器 playground |
| `GET /graphiql` | 挂在 **ui** router 上（不在 api router） | GraphiQL |

WS 子协议：`graphql-transport-ws`、`graphql-ws`（`subscriptions` `+0` 里的 `ws.protocols([...])`）。

> **纠正**：**没有** `GET /graphql/schema` 这个路由。SDL 是**签入仓库的文件** [ee/tabby-schema/graphql/schema.graphql](file:///Users/vincent/workspace/16_AI/tabby/ee/tabby-schema/graphql/schema.graphql)，用 `make update-graphql-schema` 重新生成（实跑 `cargo run --package tabby-schema --example update-schema --features=schema-language`）。要拿最新 schema 请对 `/graphql` POST 一个标准 introspection 查询。

### Authentication —— 与 REST 完全不同

GraphQL **不走** §5.1.2 那套 `authorize_request` 门（那条只拦 `/v1/`、`/v1beta/`）。它自己建 context，见 `ee/tabby-webserver/src/routes/mod.rs` · `FromAuth::build` `+0`：

1. token 来源二选一，**query 参数优先**：`?access_token=<token>`（`ee/tabby-webserver/src/axum/extract.rs` · `AuthBearer::from_request_parts` `+0`，取的是**裸 token**，不带 `Bearer ` 前缀）；或 `Authorization: Bearer <token>`（`extract_bearer_token` `+0` 严格匹配字面量 `Bearer`，**大小写敏感**，写成 `bearer` 会被当成没带 token）。
2. 先 `validate_jwt(token)`；失败再当 API token 试 `verify_auth_token(token)`，成功则**合成**一个 JWT payload。
3. 都失败或没带 token ⇒ `claims = None` ⇒ **匿名 context，HTTP 仍返回 200**。

关键差异：REST 认证失败是 **401 + 空 body**；GraphQL 认证失败是 **200 + `errors` 数组**，由每个 resolver 里的策略检查（如 `ee/tabby-schema/src/schema/mod.rs` 里 `check_user(ctx)`）抛出。做错误处理时两条链路要分开写。

订阅的 token 传法又是第三种：`connection_init` 的 `payload.authorization`，且**必须带 `Bearer ` 前缀**（`subscriptions` `+0` 直接对该值调 `extract_bearer_token`）。

### Response / Error Format

juniper 默认信封，这也是**全仓唯一**真实使用 `{ "data": ... }` 包装的地方：

```json
{
  "data": { "users": { "edges": [ { "node": { "id": "...", "email": "a@b.c" }, "cursor": "..." } ], "pageInfo": { "hasNextPage": true, "endCursor": "..." } } },
  "errors": [ { "message": "Unauthenticated", "locations": [ { "line": 2, "column": 3 } ], "path": ["users"] } ]
}
```

> 旧文档 §5.1 把这套信封当成所有 REST JSON API 的统一格式 —— REST 侧没有它（见 §5.1.3）。

### Schema 规模与根字段

实测 `schema.graphql`（已用正则解析 `type Query/Mutation/Subscription` 三个块并与 Rust 侧 `#[graphql_object]` 提取结果交叉比对，两边一致）：

| 根类型 | 字段数 |
|--------|-------|
| `Query` | **43** |
| `Mutation` | **62** |
| `Subscription` | **5** |

Subscription 全部 5 个：`createThreadAndRun`、`createThreadRun`、`createPageRun`、`createThreadToPageRun`、`createPageSectionRun`。

### Query 示例 —— Users List

```graphql
query {
  users(first: 20, after: null) {          # 完整签名: users(ids, after, before, first, last)
    edges {
      node {
        id
        email
        name
        createdAt
        isAdmin
        isOwner
        active
        isSsoUser
      }
      cursor
    }
    pageInfo { hasNextPage endCursor }
  }
}
```

`User` 是 **interface**（实现类型如 `UserSecured`），8 个字段就是上面这些。

> **纠正**：旧示例里的 `role` **不是** User 的字段 —— 角色是 `isAdmin: Boolean!` + `isOwner: Boolean!` 两个布尔；`active` 存在、写法正确。

### Query 示例 — Usage Stats

```graphql
query {
  dailyStats(
    start: "2025-01-01T00:00:00Z",
    end:   "2025-01-31T00:00:00Z",
    users: null,          # [ID!]
    languages: [PYTHON, RUST]
  ) {
    start
    end
    language
    completions
    views
    selects
  }
}
```

签名与返回类型见 `ee/tabby-schema/src/schema/mod.rs` · `daily_stats` `+0`，元素类型 `ee/tabby-schema/src/schema/analytic.rs` · `CompletionStats` `+0`（6 个字段全列出）。语言枚举是 `analytic.rs` · `Language` `+0`（GraphQL 侧大写下划线：`RUST PYTHON JAVA KOTLIN JAVASCRIPT TYPESCRIPT GO RUBY CSHARP C CPP SOLIDITY ...`）。

> **纠正**：旧示例的 `dailyStats(user:, since:, until:) { completion {...} chat {...} }` **参数名和返回结构都不对**。参数是 `start`/`end`/`users`/`languages`；返回是**扁平**的 `[CompletionStats!]!`，按 `(天, 语言)` 一行，没有嵌套的 `completion` / `chat` 分组。
> chat 维度另有独立字段 `chatDailyStats` / `chatDailyStatsInPastYear`（返回类型不同），不要复用 `dailyStats`。

### Mutation 示例 — Register User

```graphql
mutation {
  register(
    email: "alice@example.com",
    password1: "secure-password",
    password2: "secure-password",
    invitationCode: null,
    name: "Alice"
  ) {
    accessToken
    refreshToken
  }
}
```

签名见 `ee/tabby-schema/src/schema/mod.rs` · `register` `+0`。

> **纠正**：①`register` 返回 `RegisterResponse`，只有 `accessToken` / `refreshToken` 两个字段，**不返回** `{id, name, email, role}`；②密码是 `password1` + `password2` 两个参数（服务端校验一致性），不存在单个 `password`；③`invitationCode` 可选，`name` 必填。

### Subscription 示例 — Real-time Chat

```graphql
subscription {
  createThreadRun(input: {
    threadId: "VXJkZXI6dXNlcjox",
    additionalUserMessage: {
      content: "What does this function do?",
      attachments: null
    },
    options: {
      modelName: null,
      docQuery: { content: "...", searchPublic: false, sourceIds: null },
      codeQuery: { content: "...", gitUrl: null, sourceId: null, filepath: null, language: null },
      generateRelevantQuestions: false,
      debugOptions: null
    }
  }) {
    ... on ThreadAssistantMessageContentDelta { delta { content } }
    ... on ThreadAssistantMessageCompleted { run { id } }
  }
}
```

三个必须知道的形态约束：

1. 只有一个参数 `input: CreateThreadRunInput!`（`ee/tabby-schema/src/schema/thread/inputs.rs` · `CreateThreadRunInput` `+0`），子字段是 `threadId` + `additionalUserMessage: CreateMessageInput!` + `options`。**没有** `threadId:` / `message:` 平铺参数（旧示例就是这么错的）。
2. 返回类型 `ThreadRunItem` 是一个 **union，共 11 个成员**：`ThreadCreated`、`ThreadRelevantQuestions`、`ThreadUserMessageCreated`、`ThreadAssistantMessageCreated`、`ThreadAssistantMessageReadingCode`、`ThreadAssistantMessageAttachmentsCodeFileList`、`ThreadAssistantMessageAttachmentsCode`、`ThreadAssistantMessageReadingDoc`、`ThreadAssistantMessageAttachmentsDoc`、`ThreadAssistantMessageContentDelta`、`ThreadAssistantMessageCompleted`。因此**必须用 `... on` 内联片段选类型**，不能像旧示例那样直接写 `delta` / `done` 两个字段。
3. 检索调参在 `ThreadRunOptionsInput` `+0`；要覆盖 BM25/embedding/RRF 阈值走 `debugOptions.codeSearchParamsOverride`（`CodeSearchParamsOverrideInput` `+0`，5 个 `Option` 字段：`minEmbeddingScore` `minBm25Score` `minRrfScore` `numToReturn` `numToScore`），**默认 null 表示用服务端配置**。

---

## 5.10 OpenAPI Spec & SDKs

### 运行时端点

注册在 [crates/tabby/src/serve.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby/src/serve.rs)：`SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", doc)`，并且 ui router 上有 `fallback` ⇒ **访问任意非 API 路径会 307 临时重定向到 `/swagger-ui`**（未启用 EE 管理界面时）。

| 位置 | 说明 |
|------|------|
| `GET /swagger-ui` | Swagger UI |
| `GET /api-docs/openapi.json` | 运行时生成的 spec |

> **纠正**：旧文档写的 `GET /docs` 不存在；`/docs/json` 也不存在。

### spec 由谁定义

两份 `#[derive(OpenApi)]` 结构体合并：

| 结构体 | 位置 | 登记的 paths |
|-------|------|-------------|
| `ApiDoc` | `crates/tabby/src/serve.rs` · `struct ApiDoc` `+0` | `log_event`、`completions`、`chat_completions_utoipa`、`health`、`setting` ⇒ `/v1/events`、`/v1/completions`、`/v1/chat/completions`、`/v1/health`、`/v1beta/server_setting` |
| `EEApiDoc` | [ee/tabby-webserver/src/lib.rs](file:///Users/vincent/workspace/16_AI/tabby/ee/tabby-webserver/src/lib.rs) · `EEApiDoc` `+0` | `ingestion`、`delete_ingestion_source`、`delete_ingestion`（仅 `ee` feature 下 merge） |

> **`/v1beta/models` 与 `/v1beta/chat/completions` 没登记进 `paths(...)`** ⇒ 它们是真实路由但不在 spec 里。别把 spec 当路由全集。
> `ApiDoc` 的 info 块里声明的 license 是 **Apache 2.0**，与 README / 各 crate 头的 **MIT** 不一致 —— 已知文档冲突，见第十四章。

### Ingestion API（EE，走 registration token）

三个路由在 `create` `+0` 里被 `require_registration_token`（`ee/tabby-webserver/src/routes/mod.rs` · `require_registration_token` `+0`）保护 —— 用的是**注册 token**，不是用户 JWT：

| 方法 + 路径 | 作用 |
|-----------|------|
| `POST /v1beta/ingestion` | upsert 一条文档 |
| `DELETE /v1beta/ingestion/{source}` | 删整个 source |
| `DELETE /v1beta/ingestion/{source}/{id}` | 删单条 |

请求体 [crates/tabby-common/src/api/ingestion.rs](file:///Users/vincent/workspace/16_AI/tabby/crates/tabby-common/src/api/ingestion.rs) · `IngestionRequest` `+0`，6 个字段带 `validator` 约束：

| 字段 | 约束 |
|------|------|
| `source` | 长度 1–256 |
| `id` | 长度 1–256（同 source 内唯一） |
| `title` | 长度 1–65535 |
| `body` | 无长度校验 |
| `link` | `Option<String>` |
| `ttl` | `Option<String>`，必须能被 `humantime::parse_duration` 解析（如 `"90d"`） |

响应 `IngestionResponse` `+0`：`{ id, source, message }`。落库后的 sourceId 形态是 `ingestedSource:${source}`（结构体文档注释里写明的）。

### SDK / 类型生成

```bash
# 1) 拉运行时 spec 生成 TS 类型（默认从 127.0.0.1:8080 取）
cd clients/tabby-openapi && npm run codegen:fetch

# 2) 只用仓库里已有的 spec 生成
cd clients/tabby-openapi && npm run codegen
```

生成器是 `openapi-typescript ^7.0.2`，产物 `clients/tabby-openapi/lib/tabby.d.ts`；脚本 [clients/tabby-openapi/scripts/codegen.js](file:///Users/vincent/workspace/16_AI/tabby/clients/tabby-openapi/scripts/codegen.js) 里有个 `patchOpenAiTypes`：把 `/v1/chat/completions` 的请求/响应 schema **替换成空 object 占位**（`ChatCompletionRequest` / `ChatCompletionChunk` / `ChatCompletionRequestMessage`），因为这几型来自 OpenAI 侧、不由 utoipa 生成。所以 TS 类型里 chat 载荷是**无类型**的，不要指望它做校验。

另有 `clients/tabby-openapi/compatible/index.d.ts` 做向后兼容垫片：把 `/v1/health` 的 `post` 补回来（兼容 0.2.x 及更早）、把 `/v1beta/chat/completions` 指到 `/v1/chat/completions` 的类型（兼容 0.10.x 及更早）。

> **纠正**：npm 包名是 **`tabby-openapi`**（`clients/tabby-openapi/package.json`，版本 `0.12.0-dev`），**不是** `@tabbyml/tabby-openapi`；且它的 `files` 只发布 `.d.ts`（`lib/**`、`compatible/**`），是个**纯类型包**，没有运行时 client，因此旧文档里 `npm install @tabbyml/tabby-openapi` 再当 SDK 用的说法不成立。要生成带运行时的 SDK 需自行用 openapi-generator 对着 `/api-docs/openapi.json` 生成。

---

## 5.11 Error Codes Reference

| HTTP Status | 触发点 | 响应体 |
|------------|--------|--------|
| `200` | 正常；GraphQL 业务错误**也是 200**（错误在 `errors` 数组里） | 资源 JSON |
| `400` | `/v1/completions` 生成失败或空 prompt（`CompletionError::EmptyPrompt`）；`/v1/events` 未知 `type`；JSON 反序列化失败 | **空**（REST）；纯文本（axum 默认） |
| `401` | EE 下 `/v1/*`、`/v1beta/*` 缺 token 或 token 无效 | 空 |
| `405` | 仅在 chat 路由的 utoipa 注解里声明，**代码未实现** | — |
| `415` | 缺 `Content-Type: application/json` | 纯文本 |
| `422` | 同上：注解声明、**未实现** | — |
| `429` | EE 下某用户超 120 req/min（`/v1/health` 豁免） | 空 |
| `500` | `/v1/chat/completions` 的 `chat_stream()` 返回 Err；EE 读 security setting 失败 | 空 |
| `501` | 未配置 completion / chat 模型时，对应路由的兜底 handler | 空 |
| `503` | **不存在**。`/v1/health` 是配置快照，没有 `state` 字段，也不会因模型加载中而返回 503 | — |

> 三个高频误传（`MODEL_LOADING` 错误码、带 `retry_after` 的错误体、`503 loading`）在代码里**都没有对应实现**：REST 侧所有错误都是**裸状态码 + 空 body**，原因只进服务端 `warn!` 日志。客户端要区分「模型没配」和「模型出错」，只能靠 **501 vs 400/500** 加读 `/v1/health` 的 `models.*` 是否为 `null`（见 §5.5）。

---

## 源码索引（本章）

| 主题 | 文件 |
|------|------|
| 路由注册、超时层、Swagger 挂载 | `crates/tabby/src/serve.rs`（`api_router`、`struct ApiDoc`） |
| completions handler | `crates/tabby/src/routes/completions.rs` |
| chat handler + SSE 封装 | `crates/tabby/src/routes/chat.rs` |
| events handler | `crates/tabby/src/routes/events.rs` |
| health handler | `crates/tabby/src/routes/health.rs` |
| models / server_setting handler | `crates/tabby/src/routes/models.rs`、`crates/tabby/src/routes/server_setting.rs` |
| metrics handler、`/metrics` 挂载、CORS | `crates/tabby/src/routes/metrics.rs`、`crates/tabby/src/routes/mod.rs` |
| 请求/响应类型、prompt 组装 | `crates/tabby/src/services/completion.rs`、`crates/tabby/src/services/completion/completion_prompt.rs` |
| 健康快照类型 | `crates/tabby/src/services/health.rs` |
| 事件类型与 logger | `crates/tabby-common/src/api/event.rs` |
| `x-tabby-user`、`MaybeUser`、`AllowedCodeRepository` | `crates/tabby-common/src/axum.rs`、`crates/tabby-common/src/constants.rs` |
| ingestion 契约 | `crates/tabby-common/src/api/ingestion.rs` |
| EE 路由合并、GraphQL 挂载、Bearer 解析 | `ee/tabby-webserver/src/routes/mod.rs`、`ee/tabby-webserver/src/axum/extract.rs`、`ee/tabby-webserver/src/axum/mod.rs` |
| EE 鉴权 + 限流 | `ee/tabby-webserver/src/service/mod.rs`、`ee/tabby-webserver/src/rate_limit.rs` |
| GraphQL 根 schema 与 SDL | `ee/tabby-schema/src/schema/mod.rs`、`ee/tabby-schema/graphql/schema.graphql` |
| thread 输入类型 | `ee/tabby-schema/src/schema/thread/inputs.rs` |
| 分析类型 | `ee/tabby-schema/src/schema/analytic.rs` |
| OpenAPI 客户端与 codegen | `clients/tabby-openapi/{openapi.json,package.json,scripts/codegen.js,compatible/index.d.ts}` |
| spec 落盘目标 | `Makefile`（`update-openapi-doc`、`update-graphql-schema`） |

## 本篇纠正清单（相对旧版）

| # | 旧版断言 | 实测结论 |
|---|---------|---------|
| 1 | 所有 JSON API 统一返回 `{data}` / `{error:{code,message}}` 信封 | 不存在。REST 成功返回裸资源，错误是**空 body + 状态码**；`{data}` 只属于 GraphQL（juniper） |
| 2 | `/v1/completions` 支持 `max_tokens`、`stop` | 请求体只有 7 个字段，这两个都不存在 |
| 3 | `segments.cursor_position` | `Segments` 没有该字段 |
| 4 | 响应是 `{candidate:{text, finish_reason}, model}` | 真实是 `{id, choices:[{index,text}], debug_data?, mode}`，无 candidate/finish_reason/model |
| 5 | prompt 模板有 `{language}` 变量 | `build_prompt` 只展开 `{prefix}`/`{suffix}`（`strfmt!`） |
| 6 | `/v1/chat/completions` 请求体可传 `search_query`、`code_search_params` | 请求体是 OpenAI 兼容 `CreateChatCompletionRequest`，检索调参只在 GraphQL `ThreadRunOptionsInput` |
| 7 | SSE 有 `event: message` / `event: done` | 只有无名 `data:` 帧 + keep-alive 注释；流关闭即结束 |
| 8 | `/v1/events` 的 `type` 有 `completion_time`，body 有 model/language/file_path/timestamp/metadata | 合法值只有 `view`/`select`/`dismiss`，body 只有 5 个字段；`select_kind` 在 URL query |
| 9 | `/v1/health` 返回 `state: loading\|ready\|error` + fp16/parallelism/context_length/arch=cuda | `HealthState` 无任何状态字段；`arch` 是 CPU 架构，GPU 在 `device`；快照非实时 |
| 10 | `GET /v1/models` 返回 `[{id,name,type}]` | 真实是 `GET /v1beta/models` 返回 `{completion, chat}` 字符串数组，且只在配了 HTTP `supported_models` 时非 null |
| 11 | `GET /v1/server/settings` 返回 allow_anonymous_tokens/custom_docs/link_account_enabled | 真实是 `GET /v1beta/server_setting`，单字段 `disable_client_side_telemetry`；core 硬编码 false，EE 覆盖 |
| 12 | `/metrics` 暴露 6 个 `tabby_*` 指标 | 全仓无自定义指标，只有 axum-prometheus 的 4 个 `axum_http_*` |
| 13 | `GET /graphql/schema` 取 SDL | 无此路由；SDL 是签入文件 `ee/tabby-schema/graphql/schema.graphql` |
| 14 | GraphQL `users{... role}` | User 是 interface，无 `role`，是 `isAdmin`/`isOwner`/`isSsoUser` |
| 15 | `dailyStats(user, since, until){completion chat}` | 参数 `start/end/users/languages`，返回扁平 `[CompletionStats]`；chat 另有 `chatDailyStats` |
| 16 | `register(name,email,password){id name email role}` | 参数 `email/password1/password2/invitationCode/name`，返回 `RegisterResponse{accessToken,refreshToken}` |
| 17 | `subscription { createThreadRun(threadId, message){delta done} }` | 单参数 `input`，返回 `ThreadRunItem` **union（11 成员）**，必须 `... on` |
| 18 | 运行时 Swagger 在 `/docs` | 在 `/swagger-ui` + `/api-docs/openapi.json`，且非 API 路径 307 重定向到它 |
| 19 | npm 包 `@tabbyml/tabby-openapi` 可当 SDK | 真名 `tabby-openapi`，纯 `.d.ts` 类型包；chat 载荷类型被 codegen 抹成空 object |
| 20 | 429 是「超出 rate_limit(1200)」 | 1200 是**出站** provider 限流；入站按用户 120/min、桶 240，health 豁免 |
| 21 | 模型加载中返回 503 | 不会。未配模型返回 **501**，503 在全仓 REST 侧无对应实现 |




