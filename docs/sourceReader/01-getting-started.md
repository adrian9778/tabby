# 第一章：快速开始 & 概述

## 1.1 What is Tabby?

**Tabby** 是一个自托管的开源 AI 代码助手，作为 GitHub Copilot 的开源替代方案。它由 [TabbyML](https://github.com/TabbyML/tabby) 维护，workspace 版本 `0.33.0-dev.0`（本仓库当前 `Cargo.toml` 实测）。

### 核心定位

| 维度 | 说明 |
|------|------|
| **自托管** | 无需外部 DBMS、云服务；数据落在本地 `TABBY_ROOT`（默认 `~/.tabby`，Docker 镜像里固定为 `/data`） |
| **开源优先** | 核心 API server（`crates/tabby/`）开源 |
| **企业级能力** | 通过 `ee/`（Enterprise Edition）提供 SSO/RBAC/License/Analytics 等企业功能；**注意 `ee` 是默认编译进去的**，见 §1.5 |
| **模型灵活** | 本地 GGUF（llama.cpp）、Ollama、以及 `http` 类的外部 API（OpenAI 兼容 / Mistral / Azure / HuggingFace / HuggingFace Text Generation Inference / Ollama 等） |
| **消费级硬件** | 支持 CPU / CUDA / ROCm / Metal / Vulkan 五种 device（实测 `crates/tabby/src/main.rs` · `enum Device` `+0`） |

### 核心能力

1. **代码补全** — Inline completion（FIM: Fill-in-the-Middle），由 `[completion] max_input_length` / `max_decoding_tokens` 约束生成规模
2. **对话问答** — Chat completions + RAG（检索增强生成），可关联内部代码库/文档
3. **代码搜索** — Tantivy 倒排索引 + tree-sitter AST 语义分析
4. **结构化文档索引** — Issues / Pulls / Web Pages / Codes / Docs 统一纳入检索
5. **跨 IDE 支持** — VS Code、IntelliJ 平台、Vim/Neovim、Eclipse（Eclipse 客户端仍在开发中，见 §1.7）

> 许可信息有一处**仓库内自相矛盾**：`crates/tabby/src/serve.rs` 里 `ApiDoc` 的 `info.license` 声明的是 **Apache 2.0**，而仓库根 `LICENSE` 与各 crate 头部标注的是 **MIT**。以 `LICENSE` 文件为准，详见第十四章。

## 1.2 Architecture at a Glance

```
┌──────────────────────────────────────────────────────────────┐
│                        IDE / Editor                          │
│   (VS Code / IntelliJ / Vim / Neovim / Eclipse)              │
│        ┌─────────────────────────┐                           │
│        │    tabby-agent (TS)     │  ← TypeScript Agent       │
│        │  completion + chat core │     在 IDE 进程中运行      │
│        └──────────┬──────────────┘                           │
└───────────────────┼──────────────────────────────────────────┘
                    │ HTTP / SSE / WebSocket
                    ▼
┌──────────────────────────────────────────────────────────────┐
│                     Tabby Server（单进程）                   │
│  ┌─────────────────┐         ┌─────────────────────┐        │
│  │  crates/tabby/  │ (开源)   │  ee/tabby-webserver │ (EE) │
│  │                 │          │                     │        │
│  │ /v1/completions │          │ GraphQL /subscriptions│      │
│  │ /v1/chat        │          │ + Admin UI (React)  │        │
│  │ /v1/events      │          │ + Auth/SSO/RBAC     │        │
│  │ /v1/health      │          │ + Analytics         │        │
│  │ /v1beta/models  │          │ + SMTP 发送         │        │
│  │ /metrics        │          │ + Ingestion API     │        │
│  └────────┬────────┘         └──────────┬──────────┘        │
│           │                              │                   │
│   ┌───────┴──────────────────────────────┴───────┐          │
│   │              Service Layer                  │          │
│   │  prompt builder / retrieval / indexing /     │          │
│   │  auth / license / email / cron jobs          │          │
│   └──────────────────┬───────────────────────────┘          │
└──────────────────────┼──────────────────────────────────────┘
                       │
         ┌─────────────┼─────────────┐
         ▼             ▼             ▼
  ┌───────────┐ ┌───────────┐ ┌──────────────┐
  │ llama.cpp │ │  Ollama   │ │ OpenAI compat │  ← Model Backends
  │ (GGUF)    │ │           │ │ /Mistral/Azure│
  └───────────┘ └───────────┘ └──────────────┘
```

> 两个易被架构漏掉的实体：`/metrics`（axum-prometheus HTTP 指标）和 `/v1beta/models` 都注册在开源 core 上；EE 的 SMTP **发送**逻辑在 `ee/tabby-webserver/src/service/email/`，而 `ee/tabby-email` 只是一个**不含 `Cargo.toml` 的邮件模板工程**（Handlebars 模板 + `package.json`），它不是 Rust crate。

### 开源 vs 企业版边界

| 功能 | 开源 Core | Enterprise Edition |
|------|:---------:|:------------------:|
| `/v1/completions` API | ✅ | ✅ |
| `/v1/chat/completions` API | ✅ | ✅ |
| `/v1/events`、`/v1/health`、`/v1beta/models`、`/metrics` | ✅ | ✅ |
| 本地 GGUF 模型推理 | ✅ | ✅ |
| 外部 HTTP API 模型 | ✅ | ✅ |
| 代码索引 (Tantivy) | ✅ | ✅ |
| `/v1beta/server_setting` | ✅（返回硬编码 `false`） | ✅（读真实 security setting） |
| `/v1beta/ingestion` | | ✅（走 registration token） |
| Admin UI / 用户管理 | | ✅ `ee/tabby-ui` + `ee/tabby-webserver` |
| SSO (OAuth / LDAP) | | ✅ |
| RBAC / Access Policy | | ✅ |
| Usage Analytics | | ✅ |
| License Management | | ✅ |
| Email (SMTP + 模板) | | ✅ 发送在 `ee/tabby-webserver/src/service/email/`，模板在 `ee/tabby-email` |
| Custom Documents Ingestion | | ✅ |
| Pages / Thread-to-Page | | ✅ |
| User Groups / Notifications | | ✅ |

> `/v1/*` 与 `/v1beta/*` 是否需要 Bearer token，取决于 EE webserver 是否启用 —— 纯开源 core 直接放行，EE 会拦。详见第五章 §5.1.2。

## 1.3 System Requirements

### 硬件维度：仓库能证实的只有这些

| 事实 | 出处 |
|------|------|
| 5 种推理 device：`cpu` / `cuda` / `rocm` / `metal` / `vulkan` | `crates/tabby/src/main.rs` · `enum Device` `+0`（strum 序列化即这些字符串） |
| 4 个编译期 GPU feature：`cuda`、`rocm`、`vulkan`（**没有 `metal` feature**） | `crates/tabby/Cargo.toml` · `[features]`；Metal 在 Apple 硅上由 llama.cpp 默认启用，无需开关 |
| 模型文件放在 `TABBY_ROOT` 下 | `crates/tabby-common/src/path.rs` · `tabby_root` `+0`；Docker 镜像 `ENV TABBY_ROOT=/data` |
| embedding 模型有默认值 `Nomic-Embed-Text`（`parallelism = 1`） | `crates/tabby-common/src/config.rs` · `default_embedding_config` `+0` |
| 本地模型的**配置文件**默认 `parallelism = 4`、`num_gpu_layers = 9999`、`context_size = 4096` | `crates/tabby-common/src/config.rs` · `default_parallelism` `+0` / `default_num_gpu_layers` `+0` / `default_context_size` `+0` |
| **命令行**默认 `--parallelism = 1`（与上面的 4 不是同一个默认值 —— 走 `serve --model X` 时生效的是 CLI 默认 1） | `crates/tabby/src/serve.rs` · `ServeArgs` 中 `parallelism` `+0` |
| 官方 README 的默认起步组合 | `README.md`：`--model StarCoder-1B --chat-model Qwen2-1.5B-Instruct` |
| CUDA 镜像基于 `nvidia/cuda`，CI 用 `CUDA_VERSION=12.4.1`，另出 `-cuda11` 变体（11.7.1） | `.github/workflows/docker.yml` + `docker/Dockerfile.cuda` |

> **关于「几核几 G 能跑」**：官方仓库（README、`website/docs/`）**没有**给出 CPU 核数 / 内存 / 磁盘的最低配置表，各量化档位的模型体积也没有权威清单。这类数字属于运行时经验，且与量化档位强相关，本文不编造。要看某个模型的真实 GGUF 体积与推荐显存，请查 HuggingFace 上的模型仓库（registry 里的 `model_id` 即对应组织，例如 `TabbyML/StarCoder-1B`），或按第十二章的 `[model.completion.local] num_gpu_layers` 自行调档。
> `parallelism` 是唯一能直接控制显存占用的配置项，注释里写明「increasing this number will have a significant impact on the memory requirement e.g., GPU vRAM」（`crates/tabby/src/serve.rs` · `ServeArgs` 中 `--parallelism` 的 doc comment）。

### 容量规划的两个真实杠杆

| 杠杆 | 效果 |
|------|------|
| `--parallelism N`（或 `parallelism` 配置） | 并发 slot 数 ↑ ⇒ 吞吐 ↑、显存 ↑ |
| `num_gpu_layers`（默认 9999 = 全放 GPU） | 调小 ⇒ 部分层落 CPU，显存 ↓、延迟 ↑ |
| `context_size`（默认 4096） | 调小 ⇒ KV cache 显存 ↓ |

## 1.4 Quick Install (Docker) — 推荐方式

### 镜像事实（先读这段，避免照抄社区里的错 tag）

实测 `.github/workflows/docker.yml`：

| 事实 | 结论 |
|------|------|
| `strategy.matrix.device-type: [cuda]`，`image-suffix: ""` | CI **只构建 CUDA 一个镜像** |
| 构建命令 | `cargo build --no-default-features --features cuda,prod --release --package tabby` ⇒ **镜像里是带 EE 的 `prod` 构建** |
| 基础镜像 | `nvidia/cuda:12.4.1-runtime-ubuntu22.04`（`CUDA_VERSION` 由 workflow 传入）；另有 CUDA **11.7.1** 变体，tag 后缀 `-cuda11` |
| 镜像名 | `tabbyml/tabby`（Docker Hub）与 `ghcr.io/tabbyml/tabby`；官方文档统一写 `registry.tabbyml.com/tabbyml/tabby` |
| tag 生成规则 | `docker/metadata-action`：semver `{{version}}`、`nightly`、`YYYYMMDD`；分支构建为 `<branch>-<sha>`。`latest` 不是显式写出来的条目，而是 metadata-action 默认 flavor 在高版本上自动附加的 |
| `ENTRYPOINT` | `["/opt/tabby/bin/tabby"]` ⇒ `docker run <image> serve ...` 即可，**命令不要再重复写 `tabby`** |
| `ENV TABBY_ROOT=/data` | 所以挂载点固定是 `/data` |
| ROCm | `docker/Dockerfile.rocm`（`rocm/dev-ubuntu:5.7.1`）**存在但 CI 不构建、不发布** ⇒ 用 ROCm 需自行 build |
| CPU 镜像 | **不存在**。`website/docs/quick-start/installation/docker.mdx` 与 `docker-compose.mdx` 里的 CPU tab 被显式写成 `{false && ...}`（禁用语义），只有 CUDA tab 生效 |

> **纠正旧版三处**：①`:latest` 虽然实际存在（metadata-action 默认 flavor 附加），但官方 README 与安装文档一律**不带 tag** 或带明确版本号 —— 照抄社区写法时建议写 `tabbyml/tabby` 或 `tabbyml/tabby:<版本号>`；②`tabbyml/tabby:cuda` 与 `tabbyml/tabby:rocm` **都不是真实 tag**，默认镜像本身就是 CUDA 构建，ROCm 只能自构建（见第八章同题）；③`--gpus all` 是必需项而非可选优化，因为镜像里跑的是 CUDA 二进制。

### CUDA 部署（官方推荐路径）

```bash
# 前置：安装 NVIDIA Container Toolkit
docker run -d \
  --name tabby \
  --gpus all \
  -p 8080:8080 \
  -v $HOME/.tabby:/data \
  tabbyml/tabby \
    serve \
    --model StarCoder-1B \
    --chat-model Qwen2-1.5B-Instruct \
    --device cuda

# 验证
curl http://localhost:8080/v1/health
```

SELinux 环境下官方文档建议给卷挂载加 `:Z`（`website/docs/quick-start/installation/docker.mdx` 里的 Collapse 段落）：`-v $HOME/.tabby:/data:Z`。

### ROCm 部署（需自行构建）

```bash
git clone --recurse-submodules https://github.com/TabbyML/tabby
cd tabby
docker build -f docker/Dockerfile.rocm -t tabby:rocm .

docker run -d \
  --name tabby \
  --device /dev/kfd --device /dev/dri \
  -p 8080:8080 \
  -v $HOME/.tabby:/data \
  tabby:rocm \
    serve --model StarCoder-1B --device rocm
```

### CPU / Apple 硅（不走 Docker）

CPU 与 Metal 场景请用**二进制**方式安装，官方镜像不提供 CPU 构建：

```bash
# Apple M1/M2（Homebrew，Metal 无需额外 feature）
brew install tabbyml/tabby/tabby
tabby serve --device metal --model StarCoder-1B --chat-model Qwen2-1.5B-Instruct

# 纯 CPU（源码构建，见 §1.5）
cargo run serve --model TabbyML/StarCoder-1B
```

> `website/docs/quick-start/installation/apple.md` 明确说明：M1/M2 的算力「likely to be sufficient only for individual usage」，团队共享实例建议改用 CUDA/ROCm 的 Docker 托管。

### docker-compose（生产环境推荐）

```yaml
# docker-compose.yml —— 结构对齐官方 website/docs/quick-start/installation/docker-compose.mdx
version: '3.5'

services:
  tabby:
    restart: always
    image: registry.tabbyml.com/tabbyml/tabby
    command: serve --model StarCoder-1B --chat-model Qwen2-1.5B-Instruct --device cuda
    volumes:
      - "$HOME/.tabby:/data"
    ports:
      - 8080:8080
    deploy:
      resources:
        reservations:
          devices:
            - driver: nvidia
              count: 1
              capabilities: [gpu]
```

```bash
docker compose up -d
```

> 官方 compose 模板用 `restart: always` + `count: 1`（单卡），**不是** `unless-stopped` + `named volume`。若改成 named volume（`tabby-data:/data`），注意镜像里 `TABBY_ROOT=/data` 是硬编码的，挂载目标路径不能改。

## 1.5 Quick Install (源码构建)

### 前置条件（照抄 `CONTRIBUTING.md`）

```bash
git clone --recurse-submodules https://github.com/TabbyML/tabby
# 已克隆过则补 submodule（唯一 submodule 是 crates/llama-cpp-server/llama.cpp）
git submodule update --recursive --init

# Rust 工具链（CI 固定 RUST_TOOLCHAIN=1.82.0）
rustup install stable

# protobuf 编译器是硬依赖
brew install protobuf                        # macOS
apt-get install protobuf-compiler libopenblas-dev   # Ubuntu / Debian
choco install protoc                          # Windows 11 + Chocolatey
```

> **纠正**：旧版列的 `graphviz`、`sqlite3`、`pnpm`/Node 20 都**不在** `CONTRIBUTING.md` 的前置清单里。SQLite 是 `ee/tabby-db` 通过 sqlx 编译期/运行期自带的，不需要装系统 sqlite3；`pnpm` 只在改 `ee/tabby-ui` 前端时才需要（见第十一章）。部分测试还需要 mailpit SMTP server。

### 构建 —— 注意 default features 里已经含 EE

`crates/tabby/Cargo.toml` · `[features]` 实测：

```toml
default = ["ee", "llama-cpp-server/binary"]
ee      = ["dep:tabby-webserver"]
cuda    = ["llama-cpp-server/cuda"]
rocm    = ["llama-cpp-server/rocm"]
vulkan  = ["llama-cpp-server/vulkan"]
prod    = ["ee", "tabby-webserver/prod"]
```

| 目的 | 命令 |
|------|------|
| 默认构建（**含 EE**） | `cargo build --release` |
| 只要开源核心（**不含 EE**） | `cargo build --release --no-default-features` |
| CUDA + EE 生产构建（镜像用的就是这条） | `cargo build --no-default-features --features cuda,prod --release --package tabby` |
| ROCm + EE | `cargo build --no-default-features --features rocm,prod --release --package tabby` |
| Vulkan | `cargo build --release --features vulkan` |
| Apple Metal | 无需 feature，`cargo build --release` 即可（llama.cpp 在 macOS 默认启用 Metal） |

> **纠正**：旧版把 `cargo build --release --features ee` 写成「包含企业版」的额外动作 —— 实际上 `ee` 已在 `default` 里，加不加都一样；反过来，**想「不含企业版」必须显式 `--no-default-features`**。另外 CUDA 镜像用的是 `--no-default-features --features cuda,prod`，`prod` 与 `ee` 的差异见第十一章。

### 下载模型 + 启动

`tabby` 只有两个子命令：`serve`、`download`（`crates/tabby/src/main.rs` · `enum Commands` `+0`）。

```bash
# 一步到位（serve 内部会自动下载缺失模型，见 serve.rs · download_model_if_needed）
cargo run serve --model TabbyML/StarCoder-1B --device cuda
#          ↑ 注意是位置参数 `serve`，不是 `--serve`（旧版写法会直接报错）

# 显式预下载（--model 必填；可选 --prefer-local-file）
cargo run download --model TabbyML/StarCoder-1B
./target/release/tabby download --model TabbyML/StarCoder-1B
./target/release/tabby serve   --model TabbyML/StarCoder-1B --device cuda
```

`download` 子命令的参数就两个（`crates/tabby/src/download.rs` · `DownloadArgs` `+0`）：`--model <id>`（必填）、`--prefer-local-file`（默认 false，跳过远端校验）。模型 id 形态是 `<registry-org>/<model_id>`，registry 为 `TabbyML`；写错会 panic 并提示去查 `https://github.com/TabbyML/registry-tabby`（`crates/tabby-common/src/registry.rs` · `get_model_info` `+0` 的 panic 文案）。

常用 `serve` 参数（`crates/tabby/src/serve.rs` · `ServeArgs` `+0`）：

| 参数 | 默认 | 说明 |
|------|------|------|
| `--model` | `None` | completion 模型 id；不配则 `/v1/completions` 恒 501 |
| `--chat-model` | `None` | chat 模型 id；不配则 `/v1/chat/completions` 恒 501 |
| `--host` | `0.0.0.0` | |
| `--port` | `8080` | |
| `--device` | `cpu` | `cpu`/`cuda`/`rocm`/`metal`/`vulkan` |
| `--chat-device` | 同 `--device` | `requires("chat_model")` —— 不配 `--chat-model` 时传它会直接被 clap 拒绝 |
| `--parallelism` | `1` | 显存敏感，见 §1.3 |

## 1.6 First Login & Registration

首次启动后访问 `http://localhost:8080`（EE 构建）：

1. **注册管理员** —— `ee/tabby-ui` 的注册页由 GraphQL 的 `isAdminInitialized` 控制显示；服务端 `is_admin_initialized` 的真实判据是**「库里是否已有 admin 用户」**（`ee/tabby-webserver/src/service/auth.rs` · `is_admin_initialized` `+0`：`list_admin_users()` 非空）。第一个注册的人自动成为 admin（`register` 里 `!is_admin_initialized` 决定 `is_admin`）。
2. **后续注册需要邀请码** —— 这是最容易误解的点。`ee/tabby-webserver/src/service/auth.rs` · `check_invitation` `+0` 的逻辑是：
   - admin 尚未初始化 ⇒ **不需要**邀请码（放行创建管理员）；
   - admin 已初始化 ⇒ 必须带 `invitationCode`，且邀请码的 email 必须与注册 email **完全一致**，否则报 `Invitation code is not valid` / `Invitation code is not for this email address`；
   - demo mode 下 admin 已初始化则直接拒绝注册。
3. **邀请入口** —— Admin → Users 里创建 invitation（GraphQL `createInvitation(email)`），或 `requestInvitationEmail` 走邮件；`invitations` 查询可列出现有邀请码。
4. **自助注册（可选）** —— `allow_self_signup` 的真实判据是 **`SMTP 已配置 && security_setting.allowed_register_domain_list 非空`**（`ee/tabby-webserver/src/service/auth.rs` · `allow_self_signup` `+0`）。两者缺一，注册页就不会给出自助入口。
5. **配模型** —— Admin → Model Config 配置 completion / chat / embedding 三类模型。

> **纠正**：旧版「首页显示 Register 页面（仅首次，此后关闭注册入口）」不准确。注册入口**没有被关闭**，而是从「谁都能注册」变成「要么有匹配邮箱的邀请码，要么满足 SMTP + 白名单域名的自助条件」。
> 密码存储：Argon2（`ee/tabby-webserver/src/service/auth.rs` · `password_hash` `+0`，随机 salt）。

## 1.7 Connect Your IDE

所有客户端共享同一个 TypeScript agent 内核 `clients/tabby-agent/`，客户端配置统一落在 **`~/.tabby-client/agent/config.toml`**（注意与**服务端**的 `~/.tabby/` 是两个不同目录）：

```toml
[server]
endpoint = "http://localhost:8080"
token = "your-auth-token"        # EE 下必填：Admin → 用户页生成 Personal Token
```

### VS Code / VS Insiders

| 渠道 | 标识 |
|------|------|
| Visual Studio Marketplace | `TabbyML.vscode-tabby` |
| Open VSX（VSCodium 等） | `TabbyML/vscode-tabby` |
| 命令行 | Quick Open 里粘贴 `ext install TabbyML.vscode-tabby` |

装好后手动填 endpoint + token（EE 需要 token 才能过 §5.1.2 的鉴权）。

### IntelliJ Platform（IDEA / PyCharm / GoLand / Android Studio …）

- JetBrains Marketplace：插件 id **22379**（`https://plugins.jetbrains.com/plugin/22379-tabby`）
- 兼容性：**build 2023.1 或更新**的任意 IntelliJ Platform IDE（`clients/intellij/README.md`）
- 装完点状态栏的 Tabby 图标 → `Open Settings...` 填 server endpoint

### Vim / Neovim

发布用仓库名是 **`TabbyML/vim-tabby`**（主仓库源码在 `clients/vim/`）。前置要求：LSP client（Neovim 内置 + `nvim-lspconfig`）**且** Neovim 或 Vim 9.0+ 带 `+textprop`（ghost text 渲染依赖它）。

```lua
-- ~/.config/nvim/init.lua（Lazy.nvim 示例）
require("lazy").setup({
  {
    "TabbyML/vim-tabby",
    lazy = false,
    dependencies = { "neovim/nvim-lspconfig" },
    init = function()
      vim.g.tabby_agent_start_command = {"npx", "tabby-agent", "--stdio"}
      vim.g.tabby_inline_completion_trigger = "auto"
    end,
  },
})
```

用 `:LspInfo` 确认 Tabby LSP 已连上。

> **纠正**：旧版写的 `Plug 'TabbyML/tabby-vim'` 仓库名不存在（真实是 `vim-tabby`，不是 `tabby-vim`）；「手动配置 LSP client 指向 Tabby LSP endpoint」也不准确 —— vim-tabby 自己在本地用 `npx tabby-agent --stdio` 起 agent，再由 agent 去连 HTTP 服务端，**不存在**「编辑器直连 server 的 LSP endpoint」。

### Eclipse

`clients/eclipse/` 存在且源码完整，但 README 第一行写明 **「This repository is currently undergoing extensive development」**，没有 Marketplace 发布物，只能按 README 用 PDE 自行 import `plugin` + `feature` 两个目录、`pnpm turbo build` 后 `Launch an Eclipse application`，或导出 deployable feature ZIP 再 `Help → Install New Software...`。**当前不建议把它当生产客户端**。

## 1.8 Verify Installation

```bash
# Health check —— 返回的是「配置快照」，不是实时状态机
curl http://localhost:8080/v1/health
```

真实响应形态（`crates/tabby/src/services/health.rs` · `HealthState` `+0`，逐字段实测）：

```json
{
  "model": "StarCoder-1B",
  "device": "cuda",
  "cuda_devices": ["NVIDIA GeForce RTX 3060"],
  "models": {
    "completion": { "local": { "model_id": "StarCoder-1B", "device": "cuda", "cuda_devices": ["NVIDIA GeForce RTX 3060"] } },
    "embedding":  { "local": { "model_id": "Nomic-Embed-Text", "device": "cuda" } }
  },
  "arch": "x86_64",
  "cpu_info": "12th Gen Intel Core i7-12700",
  "cpu_count": 20,
  "version": {
    "build_date": "2026-08-12",
    "build_timestamp": "2026-08-12T03:41:07.123456789Z",
    "git_sha": "f05db40e...",
    "git_describe": "v0.33.0-dev.0"
  },
  "webserver": true
}
```

判读要点：

| 想确认 | 看哪个字段 |
|-------|-----------|
| completion 模型是否配上了 | `models.completion` 是否为 `null`（为 null ⇒ `/v1/completions` 恒 **501**） |
| chat 模型是否配上了 | `models.chat`（顶层 `chat_model` / `chat_device` 三个字段源码注释已标「slated for future deprecation」） |
| embedding 用的什么 | `models.embedding`（必填，默认 `Nomic-Embed-Text`） |
| 是不是 CUDA 二进制 | `device`；`cuda_devices` 为空数组只代表 NVML 不可用（macOS / 未挂 GPU 的容器），**不代表**配置错 |
| 线上跑的是哪个构建 | `version.git_describe` / `git_sha` |
| CPU 架构 | `arch` 是 `std::env::consts::ARCH`（`x86_64` / `aarch64`），**不是** GPU 后端 |

> **纠正**：旧版的 `{"state": "ready", "model": ..., "chat_model": ..., "arch": "cuda"}` 里，**`state` 字段不存在**（没有 `loading`/`ready`/`error` 状态机），也没有 `error` / `fp16` / `parallelism` / `context_length` / `fast_attention` 这些键；`arch` 的含义被误标成了 GPU 后端。所以「模型加载中会返回 503」也是不成立的 —— `/v1/health` 只要进程活着就返回 200。详见第五章 §5.5。

```bash
# Test completion —— 请求体按 CompletionRequest 真实字段
curl http://localhost:8080/v1/completions \
  -H "Content-Type: application/json" \
  -d '{
    "language": "python",
    "segments": {
      "prefix": "def fibonacci(n):\n    ",
      "suffix": "\n    return fibonacci(n-1) + fibonacci(n-2)"
    }
  }'
# 期望 200：{"id":"cmpl-<uuid>","choices":[{"index":0,"text":"..."}],"mode":"standard"}
# EE 下需加 -H "Authorization: Bearer <token>"，否则 401
```

> **纠正**：旧示例带了 `segments.cursor_position` —— **`Segments` 里没有这个字段**；同时顶层的 `model` 也不需要传。响应侧旧版写成 `{"id","model","choices":[{"index","text"}]}` 也是过时的（无 `model`，但多了 `mode`；`choices[].index` 恒为 0）。

```bash
# Test chat —— OpenAI 兼容载荷（REST 侧不做 RAG）
curl -N http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "messages": [
      {"role": "user", "content": "What is Rust?"}
    ]
  }'
# 返回 SSE：只有 data: 行，没有 event: 名；-N 关闭 curl 缓冲才能看到逐字流
```

```bash
# 模型列表（仅当配了 HTTP 模型 + supported_models 时非 null）
curl http://localhost:8080/v1beta/models

# 服务端开关
curl http://localhost:8080/v1beta/server_setting

# Prometheus 指标（注意：只有 axum_http_* 四个 HTTP 指标，没有 tabby_* 业务指标）
curl -s http://localhost:8080/metrics | head

# 交互式 API 文档
open http://localhost:8080/swagger-ui
```

> **纠正**：旧版在验证清单里写了 `POST /v1/events` 用 `type: "completion_time"` —— 合法值只有 `view` / `select` / `dismiss`（`crates/tabby-common/src/api/event.rs` · `LogEventRequest` `+0`）。`/docs` 也不是文档入口，真实路径是 `/swagger-ui` + `/api-docs/openapi.json`。

## 1.9 Configuration Quick Reference

编辑服务端配置 `~/.tabby/config.toml`（路径由 `crates/tabby-common/src/path.rs` · `tabby_root` `+0` 派生，`TABBY_ROOT` 环境变量可覆盖）：

```toml
# ---- 外部 HTTP 模型（kind 是闭集，写错当场 panic，合法清单见第十二章）----
[model.completion.http]
kind = "llama.cpp/completion"     # api_endpoint 填 llama-server 根地址，引擎会自动拼 /completion
api_endpoint = "http://localhost:8088"

[model.chat.http]
kind = "openai/chat"              # OpenAI 兼容服务一律用这个
api_endpoint = "http://localhost:11434/v1"
model_name = "Qwen2-1.5B-Instruct"

[model.embedding.http]
kind = "llama.cpp/embedding"
api_endpoint = "http://localhost:8089"

# ---- 本地 GGUF 模型（与 http 二选一；注释掉的是默认值）----
# [model.completion.local]
# model_id = "StarCoder-1B"
# parallelism = 4            # 配置文件默认 4；CLI --parallelism 默认 1
# num_gpu_layers = 9999
# enable_fast_attention = null   # Option<bool>，null 表示由 llama.cpp 自行决定
# context_size = 4096
# additional_stop_words = []     # Option<Vec<String>>

# ---- 服务端行为 ----
[server]
completion_timeout = 30      # /v1/completions 的 TimeoutLayer 秒数

# ---- 补全规模与检索参数 ----
[completion]
max_input_length = 1536      # prompt 最大 token 数
max_decoding_tokens = 64     # 最大生成 token 数

# [completion.code_search_params]   # RAG 检索阈值，答案引擎另有一套默认值
# min_embedding_score = 0.75
# min_bm25_score = 8.0
# min_rrf_score = 0.028
# num_to_return = 20
# num_to_score = 40

# ---- embedding ----
# [embedding]
# max_input_length = 5120
```

> `kind` 的合法值、各段的完整默认值与约束以 **第十二章：配置参考手册** 为准；本文只给最小可用骨架。

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
| 排障 | [第十四章：FAQ](./14-faq-troubleshooting.md) |

---

## 本篇纠正清单（相对旧版）

| # | 旧版断言 | 实测结论 |
|---|---------|---------|
| 1 | `docker run tabbyml/tabby:latest` / `:cuda` / `:rocm` 三种 tag | CI matrix 只有 `cuda`，镜像名后缀区分；`:cuda`、`:rocm` **不是**真实 tag，ROCm 需自建 |
| 2 | 存在 CPU Docker 镜像与部署段 | **无 CPU 镜像**；官方文档 CPU tab 被 `{false && ...}` 禁用。CPU/Metal 走 Homebrew 或源码构建 |
| 3 | 官方镜像是「开源核心」构建 | 镜像用 `--no-default-features --features cuda,prod` ⇒ **含 EE** |
| 4 | `docker run ... tabbyml/tabby serve` 需自带 entrypoint 名 | `ENTRYPOINT` 已是 `tabby`，命令不要再写一遍 |
| 5 | `cargo build --release` = 开源核心；`--features ee` = 含企业版 | `ee` 在 `default` features 里，默认构建**就含 EE**；要排除需 `--no-default-features` |
| 6 | `cargo run --serve --model ...` | `serve` 是子命令不是 flag：`cargo run serve --model ...` |
| 7 | 前置依赖含 `graphviz`、`sqlite3`、Node 20/pnpm | `CONTRIBUTING.md` 只要求 Rust + protobuf（macOS `brew install protobuf` / Debian `protobuf-compiler libopenblas-dev` / Windows `choco install protoc`）；pnpm 仅前端开发需要 |
| 8 | 首次注册后「关闭注册入口」 | 注册未关闭；admin 初始化后需**匹配 email 的邀请码**，自助注册条件是 **SMTP 已配 + 允许域名列表非空** |
| 9 | Vim 插件 `TabbyML/tabby-vim` | 真名 `TabbyML/vim-tabby`；且需 `+textprop`，agent 由 `npx tabby-agent --stdio` 本地起 |
| 10 | Eclipse 已可正常安装使用 | `clients/eclipse/README.md` 首行标注「undergoing extensive development」，无发布物 |
| 11 | `/v1/health` 返回 `state: ready`、`arch: cuda` | 无 `state`；`arch` 是 CPU 架构，GPU 在 `device`；响应是配置快照，不会因模型未就绪返回 503 |
| 12 | completion 请求含 `segments.cursor_position` | `Segments` 无该字段 |
| 13 | `/metrics` 可给业务指标、文档入口是 `/docs` | 只有 `axum_http_*` 四个 HTTP 指标；文档入口是 `/swagger-ui` |
| 14 | 事件类型含 `completion_time` | 只有 `view` / `select` / `dismiss` |
| 15 | 硬件最低配置表、量化档位体积表 | 仓库内无此数据，已改为「可证实项 + 调优杠杆」，不再给编造数字 |


