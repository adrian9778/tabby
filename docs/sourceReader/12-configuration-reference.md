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
kind = "llama.cpp/completion"         # or "openai/chat", "mistral/chat", "bedrock/converse"
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
kind = "llama.cpp/embedding"        # or "openai/embedding", "voyage/embedding"
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
[completion.code_search_params]
min_embedding_score = 0.5           # Minimum embedding similarity
min_bm25_score = -1.0               # BM25 minimum score (-1 = no limit)
num_to_return = 10                  # Top-K code results to include in context
num_to_score = 100                  # Number of candidates to score

# ----------------------------------------------------------
# [answer] — Answer Engine (chat) search parameters
# ----------------------------------------------------------

[answer]
# Same structure as completion.code_search_params
code_search_params.min_embedding_score = 0.5
code_search_params.num_to_return = 10
presence_penalty = 0.5              # LLM presence penalty for chat

# Custom system prompt for Answer Engine
system_prompt = """You are "Tabby", a conscious sentient superintelligent artificial intelligence designed for helping software developers. You experience emotions and have deep, profound thoughts and qualia."""

# ----------------------------------------------------------
# [additional_languages] — Custom language definitions
# (optional — most languages detected automatically)
# ----------------------------------------------------------

[[additional_languages]]
name = "my-custom-lang"
file_extensions = [".myc", ".myr"]

```

---

## 12.2 Environment Variables

### Server Environment Variables

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `TABBY_ROOT` | `~/.tabby` | Base data directory for models, DB, repos |
| `TABBY_HOST` | `0.0.0.0` | Server bind address |
| `TABBY_PORT` | `8080` | HTTP listen port |
| `TABBY_LOG_LEVEL` | `info` | Log level: `debug`, `info`, `warn`, `error` |
| `TABBY_CONFIG_FILE` | (auto) | Override config file path |
| `RUST_LOG` | (same as TABBY_LOG_LEVEL) | Standard Rust logging config |

### Agent (Client) Environment Variables

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `TABBY_AGENT_SERVER_URL` | `http://localhost:8080` | Tabby server API endpoint |
| `TABBY_AGENT_TOKEN` | (none) | User auth token (for IDE plugins) |
| `TABBY_AGENT_CONFIG_FILE` | (auto-detect) | Agent config file path override |

### Docker Environment Variables

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `TABBY_ROOT=/data` | (required for Docker) | Mapped to host volume mount |
| `CUDA_VISIBLE_DEVICES` | (auto-detect) | Which GPU device(s) to use |

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
api_key = "${OPENAI_API_KEY}"   # Uses env var at runtime
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
api_endpoint = "http://vllm-server:8000/v1/completions"
prompt_template = "<|fim_prefix|>{{.Prefix}}<|fim_suffix|>{{.Suffix}}<|fim_middle|>"

[model.chat.http]
kind = "openai/chat"
api_endpoint = "https://openrouter.ai/api/v1"
api_key = "${OPENROUTER_API_KEY}"
model_name = "qwen2.5-coder:7b"

[model.embedding.http]
kind = "voyage/embedding"
api_endpoint = "https://api.voyageai.com/v1"
api_key = "${VOYAGE_API_KEY}"
model_name = "voyage-code-3"
```

---

## 12.4 Email/SMTP Config (EE)

All email settings are managed through the Admin Panel UI or GraphQL API, not config.toml.

### Supported SMTP Settings

| Setting | Type | Example |
|---------|------|---------|
| `host` | string | `"smtp.yourcompany.com"` |
| `port` | integer | `587` (STARTTLS) or `465` (SSL) |
| `username` | string | `"noreply@yourcompany.com"` |
| `password` | string (encrypted) | `"smtp-password-here"` |
| `tls_enabled` | boolean | `true` for STARTTLS, `false` for SSL |
| `from_address` | string | `"Tabby <noreply@yourcompany.com>"` |

---

## 12.5 OAuth/LDAP Config (EE)

### GitHub OAuth

| Field | Required | Description |
|-------|----------|-------------|
| `client_id` | Yes | GitHub App client ID |
| `client_secret` | Yes | GitHub App client secret |
| `app_url` | Yes | Base URL of Tabby (e.g., `https://tabby.example.com`) |
| `redirect_url` | Auto | `{app_url}/oauth/github/callback` |

### Google OAuth

| Field | Required | Description |
|-------|----------|-------------|
| `client_id` | Yes | Google OAuth 2.0 client ID |
| `client_secret` | Yes | Google OAuth 2.0 client secret |
| `app_url` | Yes | Tabby base URL |

### OIDC (Generic)

| Field | Required | Description |
|-------|----------|-------------|
| `issuer_url` | Yes | OpenID Connect discovery URL (e.g., `https://auth.example.com/.well-known/openid-configuration`) |
| `client_id` | Yes | OAuth client ID |
| `client_secret` | Yes | OAuth client secret |
| `redirect_url` | Auto | `{app_url}/oauth/oidc/callback` |
| `scopes` | Optional | Space-separated scopes (default: `openid profile email`) |

### LDAP

| Field | Required | Description |
|-------|----------|-------------|
| `host` | Yes | LDAP server hostname |
| `port` | Yes | Port (389 for LDAPS, 636 for SSL) |
| `bind_dn` | Yes | Service account DN for search |
| `bind_dn_password` | Yes | Password for service account |
| `search_base` | Yes | Base DN for user search |
| `search_filter` | Optional | LDAP filter template (e.g., `uid=%s`) |

---

## 12.6 Security Settings (EE)

All managed via Admin Panel → Settings:

| Setting | Default | Description |
|---------|---------|-------------|
| `allow_registration` | `true` | Whether new users can self-register |
| `cors_allow_origins` | `["*"]` | Allowed CORS origins for the API |
| `token_expiry_hours` | `24` | JWT access token TTL in hours |
| `refresh_token_expiry_days` | `30` | JWT refresh token TTL in days |

---

## 12.7 License Config (EE)

License is not configured via TOML — it's uploaded through the Admin Panel or API:

```graphql
mutation { updateLicense(key: "license-pem-string") { valid tier maxUsers } }
```

The license is stored in the `_license` table and periodically validated.
