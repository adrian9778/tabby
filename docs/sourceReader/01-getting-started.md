# 第一章：快速开始 & 概述

## 1.1 What is Tabby?

**Tabby** 是一个自托管的开源 AI 代码助手，作为 GitHub Copilot 的开源替代方案。它由 [TabbyML](https://github.com/TabbyML/tabby) 维护，版本 `0.33.0-dev.0`（截至 2026-08）。

### 核心定位

| 维度 | 说明 |
|------|------|
| **自托管** | 无需 DBMS、云服务；数据完全掌握在自己手中 |
| **开源优先** | 核心 API server（`crates/tabby/`）采用 MIT 许可开源 |
| **企业级能力** | 通过 `ee/`（Enterprise Edition）提供 SSO/RBAC/License/Analytics 等企业功能 |
| **模型灵活** | 支持本地 GGUF 模型、Ollama、OpenAI-compatible API、Azure/Mistral/HuggingFace 等十余种后端 |
| **消费级 GPU** | 最低支持消费者级显卡即可运行 |

### 核心能力

1. **代码补全** — Inline completion（FIM: Fill-in-the-Middle），延迟 < 200ms
2. **对话问答** — Chat completions + RAG（检索增强生成），可关联内部代码库/文档
3. **代码搜索** — Tantivy 倒排索引 + tree-sitter AST 语义分析
4. **结构化文档索引** — Issues/Pulls/Web Pages/Codes 统一纳入检索
5. **跨 IDE 支持** — VS Code、IntelliJ/IDEA、Vim/Neovim、Eclipse

---

## 1.2 Architecture at a Glance

```
┌──────────────────────────────────────────────────────────────┐
│                        IDE / Editor                          │
│   (VS Code / IntelliJ / Vim / Eclipse)                       │
│        ┌─────────────────────────┐                           │
│        │    tabby-agent (TS)     │  ← TypeScript Agent       │
│        │  completion + chat core │     在 IDE 进程中运行      │
│        └──────────┬──────────────┘                           │
└───────────────────┼──────────────────────────────────────────┘
                    │ HTTP / SSE / WebSocket
                    ▼
┌──────────────────────────────────────────────────────────────┐
│                     Tabby Server                             │
│  ┌─────────────────┐         ┌─────────────────────┐        │
│  │  crates/tabby/  │ (开源)   │  ee/tabby-webserver │ (EE) │
│  │                 │          │                     │        │
│  │ /v1/completions │          │ GraphQL API         │        │
│  │ /v1/chat        │          │ + Admin UI (React)  │        │
│  │ /v1/events      │          │ + Auth/SSO/RBAC     │        │
│  │ /v1/health      │          │ + Analytics         │        │
│  └────────┬────────┘         └──────────┬──────────┘        │
│           │                              │                   │
│   ┌───────┴──────────────────────────────┴───────┐          │
│   │              Service Layer                    │          │
│   │  prompt builder / retrieval / indexing /      │          │
│   │  auth / license / email / cron jobs           │          │
│   └──────────────────┬───────────────────────────┘          │
└──────────────────────┼──────────────────────────────────────┘
                       │
         ┌─────────────┼─────────────┐
         ▼             ▼             ▼
  ┌───────────┐ ┌───────────┐ ┌──────────────┐
  │ llama.cpp │ │  Ollama   │ │ OpenAI compat│  ← Model Backends
  │ (GGUF/CUDA)│ │ (local)  │ │ /Azure/Mistral│
  └───────────┘ └───────────┘ └──────────────┘
```

### 开源 vs 企业版边界

| 功能 | 开源 Core | Enterprise Edition |
|------|:---------:|:------------------:|
| `/v1/completions` API | ✅ | ✅ |
| `/v1/chat/completions` API | ✅ | ✅ |
| 本地 GGUF 模型推理 | ✅ | ✅ |
| 外部 HTTP API 模型 | ✅ | ✅ |
| 代码索引 (Tantivy) | ✅ | ✅ |
| Admin UI / 用户管理 | | ✅ `ee/tabby-webserver` |
| SSO (LDAP/OAuth) | | ✅ |
| RBAC / Access Policy | | ✅ |
| Usage Analytics | | ✅ |
| License Management | | ✅ |
| Email (SMTP/模板) | | ✅ `ee/tabby-email` |
| Custom Documents Ingestion | | ✅ |
| Web Pages & Threads | | ✅ |

---

## 1.3 System Requirements

### 推荐配置

| 场景 | CPU | GPU | RAM | 磁盘 |
|------|-----|-----|-----|------|
| **最小可用** (1B 模型, Q4) | 4 核 | NVIDIA GTX 1660 / M1 | 8 GB | 5 GB |
| **推荐** (7B 模型, Q4) | 8 核 | NVIDIA RTX 3060+ / M2 | 16 GB | 10 GB |
| **高性能** (13B+ 模型) | 16 核+ | NVIDIA RTX 4090 / A10 | 32 GB+ | 20 GB+ |

### GPU 支持矩阵

| 后端 | 支持状态 |
|------|---------|
| NVIDIA CUDA 11.7.1+ | ✅ (`Dockerfile.cuda`) |
| AMD ROCm | ✅ (`Dockerfile.rocm`) |
| Apple Metal (M1/M2) | ✅ 内置 |
| Vulkan | ✅ `--features vulkan` |
| CPU-only | ✅ 默认无 GPU 即可运行 |

### 模型存储需求

不同量化等级的 GGUF 模型文件大小参考：

| 模型 | FP16 | Q8_0 | Q5_K_M | Q4_K_M | Q3_K_M |
|------|------|------|--------|--------|--------|
| StarCoder-1B | 2 GB | 1.2 GB | 0.8 GB | 0.7 GB | 0.6 GB |
| CodeLlama-7B | 14 GB | 7.5 GB | 4.9 GB | 4.1 GB | 3.4 GB |
| Qwen2-1.5B-Instruct | 3 GB | 1.6 GB | 1.0 GB | 0.9 GB | 0.7 GB |

---

## 1.4 Quick Install (Docker) — 推荐方式

### CPU-only 部署

```bash
docker run -d \
  --name tabby \
  -p 8080:8080 \
  -v $HOME/.tabby:/data \
  tabbyml/tabby:latest \
  serve --model StarCoder-1B --device cpu

# 验证安装
curl http://localhost:8080/v1/health | jq
```

### NVIDIA GPU 部署

```bash
docker run -d \
  --name tabby \
  --gpus all \
  -p 8080:8080 \
  -v $HOME/.tabby:/data \
  tabbyml/tabby:latest \
  serve --model StarCoder-1B --device cuda

# 或使用 CUDA 专用镜像 (性能更好)
docker run -d \
  --name tabby \
  --gpus all \
  -p 8080:8080 \
  -v $HOME/.tabby:/data \
  tabbyml/tabby:cuda \
  serve --model StarCoder-1B --device cuda
```

### AMD GPU 部署

```bash
docker run -d \
  --name tabby \
  --device /dev/kfd --device /dev/dri \
  -p 8080:8080 \
  -v $HOME/.tabby:/data \
  tabbyml/tabby:rocm \
  serve --model StarCoder-1B --device rocm
```

### docker-compose (生产环境推荐)

```yaml
# docker-compose.yml
version: "3.8"
services:
  tabby:
    image: tabbyml/tabby:latest
    command: serve --model StarCoder-1B --device cuda
    volumes:
      - tabby-data:/data        # 持久化数据
    ports:
      - "8080:8080"             # API 端点
    deploy:
      resources:
        reservations:
          devices:
            - driver: nvidia
              count: 1
              capabilities: [gpu]
    restart: unless-stopped

volumes:
  tabby-data:
```

```bash
docker compose up -d
```

---

## 1.5 Quick Install (源码构建)

### 前置条件

```bash
# Rust 工具链 (1.70+)
rustup install stable

# Node.js 20+ / pnpm 8+ (EE UI 开发需要)
# macOS
brew install protobuf

# Ubuntu/Debian
apt install protobuf-compiler libopenblas-dev graphviz sqlite3
```

### 构建

```bash
git clone --recurse-submodules https://github.com/TabbyML/tabby
cd tabby

# 开源核心 (不含企业版)
cargo build --release

# 包含企业版
cargo build --release --features ee
```

### 下载模型并启动

```bash
# 自动下载模型到 ~/.tabby/models/
cargo run --serve --model StarCoder-1B --device cuda

# 或直接用二进制
./target/release/tabby download --model StarCoder-1B
./target/release/tabby serve --model StarCoder-1B --device cuda
```

---

## 1.6 First Login & Registration

首次启动 Tabby server 后：

1. **访问 Web UI** — 浏览器打开 `http://localhost:8080`
2. **管理员注册** — 首页显示 "Register" 页面（仅首次，此后关闭注册入口）
3. **邀请用户** — Admin Panel → Users → Invite，通过邮件链接或手动分享注册码注册新成员
4. **配置模型** — Admin Panel → Model Config → 选择 completion/chat/embedding 模型

> **注意**: `--chat-model` 参数可选。提供 chat model 可启用 Answer Engine（RAG 对话）。

---

## 1.7 Connect Your IDE

### VS Code / VS Insiders

```
Marketplace: TabbyML.vscode-tabby
快捷安装: Ctrl+Shift+X → 搜索 "Tabby" → Install
```

安装后自动检测 `http://localhost:8080`，在编辑器中即可看到 inline completion（灰色建议文本）。

### IntelliJ Platform (IntelliJ/PyCharm/WebStorm/Goland...)

```
Plugins Marketplace: 搜索 "Tabby" 
或从 https://plugins.jetbrains.com/plugin/22379-tabby
```

### Vim / Neovim

```vim
" vimrc - 安装方式取决于你的 plugin manager
Plug 'TabbyML/tabby-vim'

" 或手动配置 LSP client，指向 Tabby LSP endpoint
```

---

## 1.8 Verify Installation

```bash
# Health check — 确认模型加载成功
curl http://localhost:8080/v1/health

# 预期响应:
{
  "state": "ready",
  "model": "tabbyml/StarCoder-1B",
  "chat_model": "tabbyml/Qwen2-1.5B-Instruct",
  "arch": "cuda",
  ...
}

# Test completion
curl http://localhost:8080/v1/completions \
  -H "Content-Type: application/json" \
  -d '{
    "language": "python",
    "segments": {
      "prefix": "def fibonacci(n):\n    ",
      "suffix": "",
      "cursor_position": 27
    }
  }'

# Test chat
curl http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "tabbyml/Qwen2-1.5B-Instruct",
    "messages": [
      {"role": "user", "content": "What is Rust?"}
    ]
  }'

# Prometheus metrics (需 Grafana/其他监控集成)
curl http://localhost:8080/metrics
```

---

## 1.9 Configuration Quick Reference

编辑配置文件 `~/.tabby/config.toml`：

```toml
# 模型配置
[model.completion.http]
kind = "llama.cpp/completion"
api_endpoint = "http://localhost:8888"

# 或本地模型
# [model.completion.local]
# model_id = "StarCoder-1B"
# parallelism = 4
# num_gpu_layers = 9999

# Chat 模型
[model.chat.http]
kind = "openai/chat"
api_endpoint = "http://localhost:8888"
model_name = "Qwen2-1.5B-Instruct"

# Embedding 模型
[model.embedding.http]
kind = "llama.cpp/embedding"
api_endpoint = "http://localhost:8888"

# 服务器超时设置
[server]
completion_timeout = 30  # completion API 超时秒数

# 补全参数
[completion]
max_input_length = 1536   # prompt 最大 token 数
max_decoding_tokens = 64  # 最大生成 token 数
```

> 完整配置参考见 **第十二章：配置参考手册**。

---

## 1.10 Next Steps

| 你接下来想... | 跳转章节 |
|-------------|---------|
| 理解整体架构设计 | [第二章：架构总览](./02-architecture.md) |
| 了解所有 Rust crates 职责 | [第三章：核心 Crates](./03-core-crates.md) |
| 部署企业版功能 | [第四章：企业版 (EE)](./04-enterprise-edition.md) |
| 查看 API 文档 | [第五章：API 参考](./05-api-reference.md) |
| 配置更多模型后端 | [第六章：模型提供商集成](./06-model-providers.md) |
| 部署到生产环境 | [第八章：部署指南](./08-deployment.md) |
| 管理用户和权限 | [第九章：系统管理](./09-administration.md) |
