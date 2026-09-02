# 第六章：模型提供商集成

## 6.1 Model Config Format

Tabby 通过 `config.toml` 管理所有模型配置，核心结构为三层：completion / chat / embedding。

```toml
[model.completion.local]      # or [model.completion.http]
model_id = "StarCoder-1B"     # (local only)
parallelism = 4
num_gpu_layers = 9999

[model.chat.http]              # or [model.chat.local]
kind = "openai/chat"
api_endpoint = "http://localhost:11434"
model_name = "qwen2.5:1.5b"

[model.embedding.http]         # or [model.embedding.local]
kind = "llama.cpp/embedding"
api_endpoint = "http://localhost:8888"
```

### 配置来源优先级

| 来源 | 优先级 | 说明 |
|------|--------|------|
| CLI flags (`--model`) | High | 命令行直接指定 |
| `~/.tabby/config.toml` | Medium | 持久化配置 |
| Admin Panel UI (EE) | Low | Web 管理界面修改（覆盖 config.toml） |

---

## 6.2 Local Models (GGUF / llama.cpp)

### 工作原理

```
config.toml [model.completion.local]
    → tabby download --model <model-id>
    → GGUF files in ~/.tabby/models/<owner>_<name>/ggml/
    → llama-cpp-server crate launches process
    → HTTP server at localhost:{port} for inference
```

### Model Directory Structure (MODEL_SPEC.md)

```
~/.tabby/models/tabbyml_starcoder-1b/
├── tabby.json                # Model metadata (prompt_template, chat_template)
└── ggml/
    └── model-00001-of-00001.gguf   # Binary model weights (GGUF format)
```

### tabby.json 规范

```json
{
  "prompt_template": "<PRE>{prefix}<SUF>{suffix}<MID>",
  "chat_template": "<s>{% for message in messages %}..."
}
```

- `prompt_template` — FIM (Fill-in-the-Middle) 推理必需的模板
- `chat_template` — 对话模型必需的 Jinja 模板

### GGUF 量化等级推荐

| 量化 | 质量 | 内存需求 (vs FP16) | 推荐场景 |
|------|------|-------------------|---------|
| **FP16** | 原始精度 | 100% | 基准测试、验证 |
| **Q8_0** | 接近 FP16 | ~50% | GPU 内存充足时 |
| **Q6_K** | 高质量 | ~43% | 生产环境推荐 |
| **Q5_K_M** | 平衡 | ~35% | 推荐（质量/体积最佳比） |
| **Q4_K_M** | 良好 | ~29% | 消费级 GPU (RTX 3060) |
| **Q3_K_M** | 可接受 | ~21% | 低内存环境 |

### llama.cpp Server Parameters

Tabby 自动配置以下参数：

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `--n-gpu-layers` | 9999 | GPU offload 层数（-1 = CPU only） |
| `--threads` | 4 | CPU 线程数 (parallelism) |
| `--context-size` | 4096 | Model context window |
| `-m` | {model_dir}/ggml/model-00001-of-*.gguf | Model file |

---

## 6.3 llamafile Integration

**llamafile** 是 Mozilla/TinyGo 推出的自包含二进制模型文件，Tabby 通过 `http-api-bindings` 直接调用。

### Config

```toml
[model.completion.http]
kind = "llamafile/completion"
api_endpoint = "http://localhost:8081"
prompt_template = "<|fim_prefix|>{{.Prefix}}<|fim_suffix|>{{.Suffix}}<|fim_middle|>"
model_name = "llamafile/qwen2.5-coder-7b"
```

### llamafile 特点

- **单文件可执行**：模型权重 + llama.cpp 推理引擎打包为一个 `.llamafile`
- **无需安装**：直接运行 `chmod +x model.llamafile`
- **跨平台**：同一二进制可在 macOS/Linux/Windows 运行
- **推荐模型**: Qwen2.5-Coder, Llama 3.1 Code

---

## 6.4 Ollama Integration

### Config

```toml
# Completion model (using Ollama as backend)
[model.completion.http]
kind = "llama.cpp/completion"
api_endpoint = "http://localhost:11434"
prompt_template = "<|fim_prefix|>{{.Prefix}}<|fim_suffix|>{{.Suffix}}<|fim_middle|>"

# Chat model
[model.chat.http]
kind = "openai/chat"
api_endpoint = "http://localhost:11434/api/chat"
model_name = "qwen2.5-coder:1.5b"
supported_models = ["qwen2.5-coder:1.5b", "qwen2.5-coder:7b"]

# Embedding model
[model.embedding.http]
kind = "llama.cpp/embedding"
api_endpoint = "http://localhost:11434/api/embeddings"
```

### 安装与配置

```bash
# Install Ollama (macOS/Linux)
curl -fsSL https://ollama.com/install.sh | sh

# Pull model
ollama pull qwen2.5-coder:1.5b
ollama pull nomic-embed-text

# Start server (default localhost:11434)
ollama serve
```

### Ollama Model Discovery

```bash
# List local models
curl http://localhost:11434/api/tags | jq

# Show model info (prompt/chat template)
curl http://localhost:11434/api/show --data '{"name":"qwen2.5-coder:1.5b"}'
```

---

## 6.5 OpenAI-Compatible APIs

支持所有 OpenAI `/v1/chat/completions` / `/v1/completions` / `/v1/embeddings` 兼容后端。

### Supported Backends

| 服务 | API Endpoint Base URL | Notes |
|------|----------------------|-------|
| **OpenAI** | `https://api.openai.com/v1` | 需要 API Key |
| **OpenRouter** | `https://openrouter.ai/api/v1` | 多模型支持 |
| **DeepInfra** | `https://deepinfra.com/inference_api` | Serverless inference |
| **Perplexity** | `https://api.perplexity.ai` | Chat-focused |
| **vLLM** (self-hosted) | `http://{host}:8000/v1` | 自托管高吞吐 |

### Config Example — OpenRouter

```toml
[model.chat.http]
kind = "openai/chat"
api_endpoint = "https://openrouter.ai/api/v1"
api_key = "sk-or-v1-..."
model_name = "qwen2.5-coder:7b"
supported_models = ["qwen2.5-coder:7b", "mistralai/mistral-7b-instruct"]

[model.completion.http]
kind = "llama.cpp/completion"
api_endpoint = "https://openrouter.ai/api/v1"
model_name = "meta-llama/llama-3.1-8b-instruct"
prompt_template = "<|begin_of_text|><|start_header_id|>user<|end_header_id|\n\n{{.Prefix}}<|eot_id|><|start_header_id|>assistant<|end_header_id|\n\n{{.Suffix}}<|eot_id|><|start_header_id|>assistant<|end_header_id|\n\n"
```

### Config Example — Azure OpenAI

```toml
[model.chat.http]
kind = "openai/chat"
api_endpoint = "https://{resource}.openai.azure.com/openai/deployments/{deployment_name}"
api_key = "{azure_api_key}"
model_name = "gpt-4"   # or model deployed name
supported_models = ["gpt-4"]

[model.embedding.http]
kind = "openai/embedding"
api_endpoint = "https://{resource}.openai.azure.com/openai/deployments/{embedding_deployment}/embeddings"
model_name = "text-embedding-3-small"
```

---

## 6.6 Mistral API Integration

### Config

```toml
[model.chat.http]
kind = "mistral/chat"
api_endpoint = "https://api.mistral.ai/v1"
api_key = "{mistral_api_key}"
model_name = "mistral-small-latest"
supported_models = ["mistral-small-latest", "mistral-medium-latest"]

[model.embedding.http]
kind = "mistral/embedding"
api_endpoint = "https://api.mistral.ai/v1"
model_name = "mistral-embed"
```

---

## 6.7 HuggingFace / Jan.ai

### HuggingFace Inference API

```toml
[model.completion.http]
kind = "llama.cpp/completion"
api_endpoint = "https://api-inference.huggingface.co/models/{model_id}"
api_key = "{hf_api_token}"
prompt_template = "<PRE>{prefix}<SUF>{suffix}<MID>"

[model.embedding.http]
kind = "llama.cpp/embedding"
api_endpoint = "https://api-inference.huggingface.co/models/{embedding_model_id}"
model_name = "{embedding_model_id}"
```

### Jan.ai (Local Server)

Jan 是一个桌面端 AI server，Tabby 可直接连接其 API：

```toml
# Jan typically runs on localhost:1337
[model.completion.http]
kind = "llama.cpp/completion"
api_endpoint = "http://localhost:1337/v1/completions"
model_name = "qwen2.5-coder:1.5b"

[model.chat.http]
kind = "openai/chat"
api_endpoint = "http://localhost:1337/v1/chat/completions"
model_name = "qwen2.5-coder:1.5b-instruct"
```

---

## 6.8 Amazon Bedrock / Voyage AI

### AWS Bedrock (Claude / Gemini)

```toml
[model.chat.http]
kind = "bedrock/converse"       # Bedrock-specific kind
api_endpoint = "https://bedrock-runtime.{region}.amazonaws.com"
api_key = "{aws_access_key}"
model_name = "anthropic.claude-3-sonnet"
```

### Voyage AI (Embedding Models)

```toml
[model.embedding.http]
kind = "voyage/embedding"        # Voyage-specific kind
api_endpoint = "https://api.voyageai.com/v1"
api_key = "{voyage_api_key}"
model_name = "voyage-code-3"     # Best for code embedding
```

---

## 6.9 Model Selection Guide

### Completion Models — Recommended by Hardware

| GPU / RAM | Q4 K_M 模型 | FP16 模型 | 延迟 (tokens/s) |
|-----------|------------|----------|-----------------|
| **CPU only** (8GB+) | qwen2.5-coder:1.5b | — | ~30 |
| **NVIDIA GTX 1660** (4GB) | qwen2.5-coder:3b | — | ~15 |
| **NVIDIA RTX 3060** (12GB) | qwen2.5-coder:7b | — | ~40 |
| **NVIDIA RTX 4090** (24GB) | codestral-22b | starcoder2:15b | ~80 |
| **Apple M1/M2** (Metal) | qwen2.5-coder:7b | — | ~25 |
| **Cloud API** | openrouter/qwen2.5-coder | any model | ~5-20 |

### Chat Models — Recommended

| 场景 | 推荐模型 | 最小配置 |
|------|---------|---------|
| 快速问答 | Qwen2.5-Coder-1.5B-Instruct | CPU / 4GB RAM |
| 通用编码 | Qwen2.5-Coder-7B-Instruct | RTX 3060 / 12GB |
| 高级推理 | Claude 3.5 Sonnet (API) | Cloud API |
| Code Review | Mistral-7B-Instruct | GTX 1660 / 8GB RAM |

### Embedding Models

| 模型 | 维度 | 推荐场景 |
|------|------|---------|
| Nomic-Embed-Text | 768 | 通用代码搜索 (本地) |
| voyage-code-3 | 1024 | 高质量代码语义搜索 (API) |
| mxbai-embed-large | 1024 | RAG/Answer Engine (本地) |

---

## 6.10 Adding a New Model Provider

如果要新增一个模型后端提供商，需要：

1. **在 `http-api-bindings/` 中实现新的 adapter**：
   ```rust
   // crates/http-api-bindings/src/completion/new_provider.rs
   impl CompletionStream for MyProvider {
       async fn completion(&self, params: CompletionParams) -> Result<CompletionResponse> {
           // Make HTTP request to the provider's API
           // Parse response into tabby-inference types
       }
   }
   ```

2. **注册 kind 字符串**：在 config parsing 时识别 `kind = "my-provider/completion"`

3. **更新 MODEL_SPEC.md** 文档，说明 prompt_template 和 chat_template

4. **添加 integration test**:
   ```rust
   #[tokio::test]
   async fn test_my_provider_completion() { ... }
   ```
