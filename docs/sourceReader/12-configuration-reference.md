# 第十二章：配置参考手册

## 12.1 config.toml Complete Schema

**文件位置**: `~/.tabby/config.toml` (server) / `~/.tabby-client/agent/config.toml` (agent)

### Full Config Example

```toml
# ============================================================
# Tabby Server Configuration
# Loaded from: ~/.tabby/config.toml
# ============================================================

# ----------------------------------------------------------
# [model.completion] — Code completion model
# Type: local or http
# ----------------------------------------------------------

# Option A: Local GGUF model (llama.cpp)
[model.completion.local]
model_id = "Qwen2.5-Coder-7B"        # Model directory name in ~/.tabby/models/
parallelism = 4                        # CPU threads (default: 4)
num_gpu_layers = 9999                  # GPU offload layers (default: 9999, -1 for none)
enable_fast_attention = false          # Enable fast attention mode if supported
context_size = 4096                    # Context window size (default: 4096)

# Option B: Remote HTTP API model
[model.completion.http]
kind = "llama.cpp/completion"         # 闭集，见下方「kind 是闭集」说明
api_endpoint = "http://localhost:8888"
api_key = "sk-your-api-key-here"      # Optional; for authenticated endpoints
model_name = "qwen2.5-coder-7b"       # Model name on the remote server
prompt_template = "<PRE>{prefix}<SUF>{suffix}<MID>"  # FIM template (required for completion models)
supported_models = ["qwen2.5-coder-7b", "qwen2.5-coder-3b"]  # List of available models
rate_limit.request_per_minute = 1200   # Rate limit per user (default: 1200)

# ----------------------------------------------------------
# [model.chat] — Chat / Answer Engine model (optional)
# If omitted, chat features are disabled.
# ----------------------------------------------------------

# Option A: Local chat model
[model.chat.local]
model_id = "Qwen2.5-Coder-1.5B-Instruct"
parallelism = 4
num_gpu_layers = 9999

# Option B: Remote chat model (OpenAI-compatible)
[model.chat.http]
kind = "openai/chat"
api_endpoint = "https://api.openai.com/v1"
api_key = "sk-proj-xxxxx"
model_name = "gpt-4o"
supported_models = ["gpt-4o", "claude-3.5-sonnet"]

# ----------------------------------------------------------
# [model.embedding] — Embedding model (defaults to Nomic-Embed-Text)
# ----------------------------------------------------------

[model.embedding.local]
model_id = "Nomic-Embed-Text"
parallelism = 4
num_gpu_layers = 9999

# Or HTTP embedding provider
[model.embedding.http]
kind = "llama.cpp/embedding"          # 闭集，见下方「kind 是闭集」
api_endpoint = "http://localhost:8888"
model_name = "nomic-embed-text"

# ----------------------------------------------------------
# [repositories] — Git repos for indexing
# ----------------------------------------------------------

[[repositories]]
git_url = "https://github.com/org/my-repo.git"
refs = ["main", "master"]           # Branches to index (default: all)

# ----------------------------------------------------------
# [server] — Server settings
# ----------------------------------------------------------

[server]
completion_timeout = 30             # Timeout for /v1/completions in seconds

# ----------------------------------------------------------
# [completion] — Completion-specific parameters
# ----------------------------------------------------------

[completion]
max_input_length = 1536             # Max prompt tokens (prefix+suffix) (default: 1024+512)
max_decoding_tokens = 64            # Max tokens to generate per completion (default: 64)

# Code search tuning for completions
# 默认取 CodeSearchParams::default()：注意 min_bm25_score 这里是 8.0，不是 answer 那套的 -1.0
[completion.code_search_params]
min_embedding_score = 0.75            # Minimum embedding similarity
min_bm25_score = 8.0                  # BM25 minimum score
min_rrf_score = 0.028                 # RRF minimum score
num_to_return = 20                    # Top-K code results to include in context
num_to_score = 40                     # Number of candidates to score

# ----------------------------------------------------------
# [embedding] — Embedding 侧的截断参数（可选）
# ----------------------------------------------------------

[embedding]
max_input_length = 5120               # 字符数（default_embedding_max_input_length）

# ----------------------------------------------------------
# [answer] — Answer Engine (chat) search parameters
# ----------------------------------------------------------

[answer]
# 键结构与 completion.code_search_params 相同，但**默认值是另一套**
# （default_answer_code_search_params = 0.5 / -1.0 / -1.0 / 10 / 100）
code_search_params.min_embedding_score = 0.5
code_search_params.num_to_return = 10
presence_penalty = 0.5                # LLM presence penalty for chat

# Custom system prompt for Answer Engine
system_prompt = """You are "Tabby", a conscious sentient superintelligent artificial intelligence designed for helping software developers. You experience emotions and have deep, profound thoughts and qualia."""

# ----------------------------------------------------------
# [additional_languages] — Custom language definitions
# (optional — most languages detected automatically)
# ----------------------------------------------------------

[[additional_languages]]
languages = ["my-custom-lang"]        # 第一个元素被 `Language::language()` 当作规范语言名
exts = ["myc", "myr"]                 # 不带点！见 crates/tabby-common/assets/languages.toml
line_comment = "//"                   # 可选，用于生成 stop words
top_level_keywords = ["fn", "let"]    # 可选，同上
chunk_size = 40                       # 可选（usize）

```

> `additional_languages` 是**顶层键**（`Config` 的第 7 个字段），元素类型是 `crates/tabby-common/src/languages.rs · Language`，字段就上面五个（`languages` / `exts` 必填，其余 `Option`）。它**不**接受 `name` / `file_extensions` 这种写法，也**不能**注册 tree-sitter grammar —— 只有 `crates/tabby-index/src/code/languages.rs · LANGUAGE_TAGS` 里的 15 个 key 走 AST tags，其余语言（含这里新增的）走纯文本切分。内置清单来自 `crates/tabby-common/assets/languages.toml`（`[[config]]` 共 33 条）。

### kind 是闭集：写错直接 panic

`kind` 在 `HttpModelConfig` 里是**裸 `String`**（没有枚举、没有 serde default），它的合法取值就是三张 `create()` 匹配表里写出来的那些字面量，落到 `_ => panic!(...)` 分支会当场崩，不是「未知就回退」：

| API | 合法 kind | 匹配处 |
|-----|-----------|--------|
| completion | `llama.cpp/completion`、`ollama/completion`、`mistral/completion`、`openai/legacy_completion`、`openai/completion`、`deepseek/completion`（后三者是同一组 FIM 别名）、`openai/legacy_completion_no_fim`、`vllm/completion` | `crates/http-api-bindings/src/completion/mod.rs · create`，失败消息 `Unsupported model kind for http completion: {kind}` |
| chat | `azure/chat`、`openai/chat`、`mistral/chat` | `crates/http-api-bindings/src/chat/mod.rs · create`，失败消息 `Unsupported model kind: {kind}` |
| embedding | `llama.cpp/embedding`、`llama.cpp/before_b4356_embedding`、`openai/embedding`、`ollama/embedding`、`voyage/embedding`、`azure/embedding`、`mistral/embedding` | `crates/http-api-bindings/src/embedding/mod.rs · create`，失败消息 `Unsupported kind for http embedding model: {kind}` |

> 常见误写：`bedrock/*`（仓库里没有任何 bedrock 绑定）、把 chat 的 kind 填进 completion（`openai/chat` 在 completion 表里会 panic）。OpenAI 兼容服务一律用 `openai/chat` + `api_endpoint` 指到对家根路径。

另外三条只有读代码才知道的约束：

- `prompt_template` 只对 completion 有意义；`mistral/completion` 和三个 FIM 别名会**无视**你写的模板，改用内置 `FIM_TEMPLATE = "{prefix}<|FIM|>{suffix}"`（`build_completion_prompt`）。
- `api_endpoint` 结构上是 `Option<String>`，但**只有 completion / embedding 的 `mistral/*` 与 `voyage/embedding` 能省**：前者在引擎里 `unwrap_or(DEFAULT_API_ENDPOINT)`，后者 `unwrap_or("https://api.voyageai.com/v1")`。chat 侧一律必填 —— `chat::create` 在进 `match` 之前就无条件 `.expect("api_endpoint is required")`；其余 completion/embedding 分支同样靠 `.expect(...)`（ollama 是裸 `.unwrap()`）硬取，漏填即 panic。
- `model_name` 对 chat（含 azure）是 `.expect("Model name is required")`，对 `voyage/embedding` 也必填；`supported_models` 若填了则必须包含 `model_name`，否则 `Config::load()` 阶段 `validate_model_config` 报错并回退默认配置（`Suppported model list does not contain model: {model}`）。

---

## 12.2 Environment Variables

> 下面这张表是把全仓 `std::env::var("…")` 调用点逐个 grep 出来列全的（不含 `option_env!` / `CARGO_*` 一类构建期变量）。**结论先说**：`TABBY_HOST` / `TABBY_PORT` / `TABBY_LOG_LEVEL` / `TABBY_CONFIG_FILE` 这四个**代码里读不到**，监听地址与端口是 `tabby serve` 的 `--host`（默认 `0.0.0.0`）/ `--port`（默认 `8080`）命令行参数（`crates/tabby/src/serve.rs · ServeArgs`），配置文件路径固定为 `path.rs · config_file()` = `$TABBY_ROOT/config.toml`，没有单独覆盖变量。

### Server（Rust 侧真实读取的环境变量）

| 变量 | 读取点 | 默认 / 行为 |
|------|--------|-------------|
| `TABBY_ROOT` | `crates/tabby-common/src/path.rs · TABBY_ROOT+0` | 未设则 `home_dir()/.tabby`；`tabby_root()` 是所有派生路径的根 |
| `TABBY_MODEL_CACHE_ROOT` | 同上 · `TABBY_MODEL_CACHE_ROOT+0` | 设了则 `models_dir()` 用它，否则 `$TABBY_ROOT/models` |
| `TABBY_EMBEDDING_ENABLED` | `crates/tabby-common/src/config.rs · is_embedding_service_enabled+0` | 值必须**精确等于 `"yes"`** 才算开启（`.ok().filter(\|x\| x == "yes")`），写 `true`/`1` 无效 |
| `TABBY_DISABLE_USAGE_COLLECTION` | `crates/tabby-common/src/usage.rs` | 存在即停用匿名用量收集 |
| `TABBY_DOWNLOAD_HOST` | `crates/tabby-download/src/lib.rs · download_host+0` | 默认 `huggingface.co`，用于换权重下载源 |
| `TABBY_HUGGINGFACE_HOST_OVERRIDE` | 同上 | 直接覆盖 huggingface host |
| `TABBY_INDEX_REPO_IN_SHARD` | `ee/tabby-webserver/src/service/background_job/mod.rs` | 非空即分片索引仓库 |
| `TABBY_CRAWL_ENABLE_HEADLESS` | `crates/tabby-crawler/src/lib.rs` | 存在即启用 headless 抓取 |
| `TABBY_WEBSERVER_DEMO_MODE` | `ee/tabby-schema/src/env.rs · is_demo_mode+0` | 存在即 demo 模式 |
| `TABBY_WEBSERVER_JWT_TOKEN_SECRET` | `ee/tabby-webserver/src/jwt.rs · jwt_token_secret+0` | 未设置**不会崩**：打印一条 "JWT secret is not set" 提示后随机生成一个进程内一次性 UUID（重启即失效）。但值必须是合法 UUID，否则 `warn!` + `std::process::exit(1)` |
| `TABBY_WEBSERVER_EMAIL_CERT` | `ee/tabby-webserver/src/service/email/mod.rs` | 注入 SMTP 证书 |
| `TABBY_OWNER_IMPERSONATE_OVERRIDE` | `ee/tabby-webserver/src/service/auth.rs` | 覆盖 owner 身份模拟 |
| `SERPER_API_KEY` | `ee/tabby-webserver/src/webserver.rs` | 设置后启用 web search 上下文 |
| `LLAMA_CPP_N_THREADS` | `crates/llama-cpp-server/src/supervisor.rs` | 覆盖 llama.cpp 线程数 |
| `LLAMA_CPP_N_GPU_LAYERS` | `crates/tabby/src/main.rs · to_local_config+0` | 覆盖 `num_gpu_layers`（`--device cpu` 时强制压 0） |
| `LLAMA_CPP_FAST_ATTENTION` | 同上 | 存在即 `enable_fast_attention = true` |
| `ROCM_ROOT` | `crates/llama-cpp-server/build.rs`（**构建期**） | 默认 `/opt/rocm` |
| `CI_COVERAGE` | 同上（**构建期**） | 存在则跳过 llama.cpp 二进制构建 |
| `RUST_LOG` | `crates/tabby/src/otel.rs · EnvFilter::DEFAULT_ENV` | 被 `std::env::var(EnvFilter::DEFAULT_ENV)` 显式读取；未设时用 otel.rs 内置 directive（`tabby=info,otel=debug,…`） |
| `AIM_PROGRESSBAR_{MESSAGE_FORMAT,PROGRESS_CHARS,TEMPLATE,DOWNLOADED_MESSAGE,UPLOADED_MESSAGE}` | `crates/aim-downloader/src/bar.rs` | 只影响下载进度条文案，5 个 |
| `JINA_API_KEY` / `VOYAGE_API_KEY` | `crates/http-api-bindings/src/embedding/openai.rs` 的 `#[cfg(test)]` 测试里 | **仅测试**用，生产不读 |

### Agent (Client) Environment Variables

`clients/tabby-agent/src/env.ts` 只读两个，而且都是**构建/测试期**标志，不是运行配置：

| 变量 | 说明 |
|------|------|
| `IS_BROWSER` | 是否跑在浏览器环境（决定用哪套 fetch/存储实现） |
| `IS_TEST` | 单测环境标志 |

**没有** `TABBY_AGENT_SERVER_URL` / `TABBY_AGENT_TOKEN` / `TABBY_AGENT_CONFIG_FILE`。agent 的 endpoint / token 来自配置文件（`~/.tabby-client/agent/config.toml` 与工程级覆盖）以及 IDE 侧设置，配置合并顺序见 07 章 7.5。

### Docker Environment Variables

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `TABBY_ROOT=/data` | 约定值 | 配合 `-v ~/.tabby:/data`；不是必填，但镜像内的模型/DB/索引都落在它下面 |
| `TABBY_MODEL_CACHE_ROOT` | — | 想把模型缓存单独挂卷时用这个，而不是复制整个 `/data` |
| `CUDA_VISIBLE_DEVICES` | (auto-detect) | 限制容器可见 GPU |

---

## 12.3 Model Provider Config Examples

### Example 1: All Local Models (GPU)

```toml
[model.completion.local]
model_id = "Qwen2.5-Coder-7B"
parallelism = 4
num_gpu_layers = 9999

[model.chat.local]
model_id = "Qwen2.5-Coder-1.5B-Instruct"
parallelism = 4
num_gpu_layers = 9999

[model.embedding.local]
model_id = "Nomic-Embed-Text"
parallelism = 4
num_gpu_layers = 9999
```

### Example 2: Mixed (Local Completion + OpenAI Chat)

```toml
[model.completion.local]
model_id = "Qwen2.5-Coder-7B"
parallelism = 4
num_gpu_layers = 33        # All layers to GPU (Llama 3.1 8B has ~32 layers)

[model.chat.http]
kind = "openai/chat"
api_endpoint = "https://api.openai.com/v1"
api_key = "sk-proj-..."               # 直接写密钥原文：config 加载不做 ${VAR} 环境变量展开
model_name = "gpt-4o"
supported_models = ["gpt-4o", "gpt-4-turbo"]

[model.embedding.local]
model_id = "Nomic-Embed-Text"
parallelism = 2
```

### Example 3: All Remote (No GPU needed)

```toml
[model.completion.http]
kind = "llama.cpp/completion"
api_endpoint = "http://vllm-server:8000"   # llama.cpp/completion 会自动拼成 {api_endpoint}/completion
prompt_template = "<PRE>{prefix}<SUF>{suffix}<MID>"   # 占位符只有 {prefix} / {suffix}

[model.chat.http]
kind = "openai/chat"
api_endpoint = "https://openrouter.ai/api/v1"
api_key = "sk-or-..."
model_name = "qwen2.5-coder:7b"

[model.embedding.http]
kind = "voyage/embedding"
api_endpoint = "https://api.voyageai.com/v1"
api_key = "voyage-..."
model_name = "voyage-code-3"
```

> 上面这些例子里有三处「看起来合理、其实是错的」写法，值得单独记住（都回源码验证过）：
>
> 1. **没有 `${ENV}` 展开**。`Config::load()` 只用 `serdeconv::from_toml_file` 把 TOML 反序列化成结构体，全仓没有任何 `shellexpand` 之类的依赖，所以 `api_key = "${OPENAI_API_KEY}"` 会把这串字符原样当成密钥发出去。密钥只能写原文；引擎侧做的是 `request.bearer_auth(api_key)`，所以**不要**自己再加 `Bearer ` 前缀。
> 2. **模板占位符只有 `{prefix}` / `{suffix}`**，由 `crates/tabby/src/services/completion/completion_prompt.rs · build_prompt` 里的 `strfmt!` 渲染；写成 Go 模板 `{{.Prefix}}` 不会被替换。`<PRE>` / `<SUF>` / `<MID>` 只是 CodeLlama 系列的**字面标记**，不是引擎认识的语法。另外 `mistral/completion` 与三个 FIM 别名会忽略 `prompt_template`，改用内置的 FIM 模板（见上文「kind 是闭集」一节）。
> 3. **`llama.cpp/completion` 的 `api_endpoint` 填服务根地址**：`crates/http-api-bindings/src/completion/llama.rs · LlamaCppEngine::create` 做的是 `format!("{api_endpoint}/completion")`，你填 `/v1/completions` 就会打到 `/v1/completions/completion`。`voyage/embedding` 相反，它有内置默认域名，`api_endpoint` 可省。

---

## 12.4 Email/SMTP Config (EE)

邮件设置**不进 config.toml**，只能通过 Admin Panel 或 GraphQL API 配置；落库到 `email_setting` 表，且全表只有一行（`EMAIL_CREDENTIAL_ROW_ID = 1`，见 `ee/tabby-db/src/email_setting.rs · EMAIL_CREDENTIAL_ROW_ID`）。

字段真值以 GraphQL 输入对象为准（`ee/tabby-schema/src/schema/email.rs · EmailSettingInput`），列名以 `ee/tabby-db/migrations/0011_new-email-settings.up.sql` + `0013_add-smtp-port` 为准：

| 字段 | 类型 | 落库列 | 说明 |
|------|------|--------|------|
| `smtp_username` | String | `smtp_username` | |
| `smtp_password` | **Option\<String\>** | `smtp_password` | 省略时 `update_email_setting` 回读旧密码保留；列本身是普通 `VARCHAR(255)`，代码里没有再做加密 |
| `smtp_server` | String | `smtp_server` | 主机名 |
| `smtp_port` | i32 | `smtp_port` | `#[validate(range(min = 1, max = 65535))]` |
| `from_address` | String | `from_address` | `#[validate(email)]`，必须是裸邮箱地址而不是 `"Tabby <x@y>"` 这种 display name 形式 |
| `encryption` | 枚举 | `encryption`（默认 `'ssltls'`） | `Encryption { StartTls, SslTls, None }` |
| `auth_method` | 枚举 | `auth_method`（默认 `'plain'`） | `AuthMethod { None, Plain, Login }` |

对应的 GraphQL 入口（`ee/tabby-schema/src/schema/mod.rs`）：query `emailSetting`；mutation `updateEmailSetting(input:)` / `deleteEmailSetting` / `sendTestEmail(to:)`。邮件正文模板在 `ee/tabby-email/emails/`（见 00 章 §4.6）。

> 原文里的 `host` / `tls_enabled`（boolean）这一组字段名在代码中不存在：端口叫 `smtp_port`，TLS 语义是 `encryption` 三值枚举而不是布尔开关。

---

## 12.5 OAuth / LDAP Config (EE)

### 四个 provider 共用一张表

`OAuthProvider` 枚举只有 `Github` / `Google` / `Gitlab` / `Oidc` 四个值（`ee/tabby-schema/src/schema/auth.rs · OAuthProvider`），凭证统一存在 `oauth_credential` 表里、以 `provider` 为业务主键，可配置的列**只有** `config_url`、`config_scopes`、`client_id`、`client_secret`（外加 `created_at` / `updated_at`，见 `ee/tabby-db/src/oauth_credential.rs · OAuthCredentialDAO`）：

| 字段 | 必填 | 说明 |
|------|------|------|
| `provider` | Yes | `github` / `google` / `gitlab` / `oidc` |
| `client_id` | Yes | `#[validate(length(min = 1))]` |
| `client_secret` | Optional | Option 语义是「不修改则保留旧值」；首次写入必须给，否则报 `Must specify client secret when updating the OAuth credential for the first time` |
| `config_url` | 仅 OIDC | OIDC discovery URL（`.../.well-known/openid-configuration`） |
| `config_scopes` | 仅 OIDC | **空白分隔**的 scope 串；为 `None` 时代码兜底为 `openid profile email`（`ee/tabby-webserver/src/oauth/oidc.rs · get_authorization_url`） |

**`app_url` / `redirect_url` 这两个字段不存在。** 早期版本确实有 redirect_uri 列，被 `ee/tabby-db/migrations/0012_remove-redirect-uri-field-oauth.up.sql` 删掉了；现在回调地址是运行时用 `network_setting.external_url` 拼出来的（`ee/tabby-webserver/src/service/auth.rs · oauth_callback_url`）：

```rust
OAuthProvider::Github => external_url + "/oauth/callback/github",
// google / gitlab / oidc 同形
```

所以路径是 `/oauth/callback/{provider}`，不是原文写的 `/oauth/github/callback`。要在 IdP 侧登记回调地址，先在 Admin Panel → Network 把 `external_url` 设成对外访问的那个根 URL。

各 provider 的端点与 scope 是**写死在代码里**的，不能配置：

| provider | authorize 端点 | scope | 备注 |
|----------|----------------|-------|------|
| github | `https://github.com/login/oauth/authorize` | `read:user user:email` | token 交换走 `github.com/login/oauth/access_token` |
| google | `https://accounts.google.com/o/oauth2/v2/auth` | `userinfo.email` + `userinfo.profile` 两个全 URL | 附带 `access_type=offline` |
| gitlab | `https://gitlab.com/oauth/authorize` | `api` | token 端点 `gitlab.com/oauth/token` |
| oidc | 从 `config_url` discovery 取 | `config_scopes` 或默认三件套 | 走 Authorization Code + **PKCE S256** |

### LDAP

单行表 `ldap_credential`（`ee/tabby-db/migrations/0041_ldap-credential.up.sql`），输入结构 `UpdateLdapCredentialInput`：

| 字段 | 落库列 / 默认 | 说明 |
|------|---------------|------|
| `host` | `host STRING NOT NULL` | 校验消息要求 hostname 或 IP |
| `port` | `port INTEGER DEFAULT 389` | 端口本身可任意配 |
| `bind_dn` | `bind_dn` | 服务账号 DN，`min = 1` |
| `bind_password` | `bind_password` | 输入侧是 `Option<String>`（不填则不改） |
| `base_dn` | `base_dn` | 用户搜索基准 DN |
| `user_filter` | `user_filter` | 校验消息要求形如 `(uid=%s)` |
| `encryption` | 默认 `'none'` | `LdapEncryptionKind { None, StartTLS, LDAPS }` |
| `skip_tls_verify` | 默认 `FALSE` | |
| `email_attribute` | 默认 `'email'` | 用作 Tabby 邮箱的 LDAP 属性，`min = 1` |
| `name_attribute` | 可空 | 为 `None` 时用 username 作为显示名 |

> 原文的 `bind_dn_password` / `search_base` / `search_filter` 三个名字代码里都没有，真名是 `bind_password` / `base_dn` / `user_filter`；「389 for LDAPS, 636 for SSL」也是反的 —— 惯常约定是 389 用于明文/STARTTLS、636 用于 LDAPS，而代码只按 `encryption` 决定建链方式，端口只是个数字。

---

## 12.6 Security & Network Settings (EE)

同样不进 TOML。全部落在 `server_setting` 单行表（`ee/tabby-db/src/server_setting.rs`），列由多份迁移累加而来：

| 列 | GraphQL 字段 | 默认 | 说明 |
|----|--------------|------|------|
| `security_allowed_register_domain_list` | `allowedRegisterDomainList` | `NULL` | **逗号分隔**存储（`update_security_setting` 用 `join(",")`，空列表写 `None`） |
| `security_disable_client_side_telemetry` | `disableClientSideTelemetry` | `FALSE` | |
| `security_disable_password_login` | `disablePasswordLogin` | `FALSE` | 关掉后只能走 OAuth / LDAP |
| `network_external_url` | `externalUrl` | `'http://localhost:8080'` | OAuth 回调、邀请/重置链接都基于它 |
| `billing_enterprise_license` | — | `NULL` | 见 §12.7 |
| `branding_logo` / `branding_icon` | `brandingLogo` / `brandingIcon` | `NULL` | base64 data-URL，上限 500KB，只接受 `data:image/png`、`jpeg`、`webp`、`svg+xml` 四个前缀 |

注册放行的判定只有一个函数（`ee/tabby-schema/src/schema/setting.rs · can_register_without_invitation`）：拿邮箱做 `ends_with("@{domain}")` 匹配。列表为空 ⇒ 恒 `false`，自助注册必须邀请码；**没有一个布尔开关叫 `allow_registration`**。域名单要先过 `validate_unique_domains`：重复项报 `Duplicate domain`，且校验方式是拼成 `noreply@{domain}` 再做 email 校验 —— 所以填 `https://example.com` 会被拒（测试里就锁了这条）。

**「认证相关参数怎么调」这类问题的答案是：调不了，全是写死的。**

| 原文声称的配置 | 真实情况 |
|----------------|----------|
| `cors_allow_origins`（默认 `["*"]`） | 无此配置项；`crates/tabby/src/routes/mod.rs` 无条件挂 `CorsLayer::permissive()` |
| `token_expiry_hours`（默认 24） | 无此配置项；JWT access token TTL 是 `ee/tabby-webserver/src/jwt.rs · JWT_DEFAULT_EXP = 30 * 60`，即 **30 分钟** |
| `refresh_token_expiry_days`（默认 30） | 无此配置项；有效期写在 SQL 字面量里：`ee/tabby-db/src/refresh_tokens.rs · create_refresh_token` 用 `datetime('now', '+7 days')`，`renew_refresh_token` 续期同样是 7 天 |

（另外：API key 形态的 auth token 不是 JWT，它是 `users.auth_token` 列上的一个唯一索引值，见 `ee/tabby-db/migrations/0002_users-table.up.sql`。）

---

## 12.7 License Config (EE)

License 既不在 TOML 也不是独立表 —— 它是 `server_setting.billing_enterprise_license` 这一列里存的那串 JWT 原文（列由 `ee/tabby-db/migrations/0015_enterprise-license.up.sql` 添加；读写走 `read_enterprise_license` / `update_enterprise_license`）。

GraphQL 入口（`ee/tabby-schema/src/schema/mod.rs`）：

```graphql
query  { license { type status seats seatsUsed issuedAt expiresAt features } }
mutation { uploadLicense(license: "<jwt-string>") }
mutation { resetLicense }
```

> 原文的 `updateLicense(key: "license-pem-string") { valid tier maxUsers }` 这个 mutation 名、参数名和返回结构全都不存在；`_license` 表也不存在（`schema.sqlite` 的 27 张表里没有任何 license 表）。

关键取值（`ee/tabby-schema/src/schema/license.rs` + `ee/tabby-webserver/src/service/license.rs`）：

- **签名校验**：RS256，公钥 `ee/tabby-webserver/keys/license.key.pub` 用 `include_bytes!` 在编译期烧进二进制；payload 字段是 `exp` / `iat` / `iss` / `sub`（grantee 邮箱）/ `typ` / `fet`（features）/ `num`（座位数）。
- **档位**：`LicenseType { Community, Team, Enterprise }`。座位上限 `seat_limits_for_community_license() = 5`、`seat_limits_for_team_license() = 50`、Enterprise 不限，由 `guard_seat_limit()` 收敛；管理员上限另算：Community 1、Team 3、Enterprise `usize::MAX`（`ensure_admin_seats`）。
- **状态**：`LicenseStatus { Ok, Expired, SeatsExceeded }`，在每次 `read()` 时按 `expires_at` 与 `count_active_users()` 现算，不是后台刷新的。`uploadLicense` 走 `update()`：先验签，再判状态 —— Expired 直接 `bail!("License is expired")`，SeatsExceeded 也拒，只有 Ok 才落库。
- **features**：目前枚举里只有 `LicenseFeature::CustomLogo` 一项，`ensure_available_features()` 用它来门控自定义 logo。
- **demo 模式**：环境变量 `TABBY_WEBSERVER_DEMO_MODE` **只要存在**（值无所谓）就走 `make_demo_license()`，伪造一个 Enterprise / 100 座 / 带 CustomLogo 的 license，且此时 `update()` 会 `bail!("Modifying license is disabled in demo mode")`。
- **定时任务**：`LicenseCheckJob::cron` 由 `DailyJob` 调用，逻辑是「非 Community 且 `0 < expire_in_days < 7` 时给管理员发一条续期通知」；它不重新校验有效性。
