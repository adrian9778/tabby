# 第七章：IDE 客户端 & 扩展

## 7.1 tabby-agent Overview

**tabby-agent** 是 Tabby 生态中运行在 IDE 进程内的 TypeScript Agent，负责所有与 Tabby server 的交互逻辑。

### Architecture

```
┌─────────────────────────────────────────────────────┐
│                    IDE Host                          │
│   VS Code / IntelliJ / Vim / Eclipse                 │
└──────────────────────┬──────────────────────────────┘
                       │
              ┌────────▼────────┐
              │ tabby-agent      │
              │ (Node.js TS)     │
              └──┬─────┬─────┬──┘
                 │     │     │
        ┌────────▼┐ ┌─▼────┐│ ┌──────────────────┐
        │Code     │ │Chat  ││ │Context           │
        │Comp.    │ │Core ││ │Providers         │
        │Pipeline │ │      ││ │ (git/ws/doc)     │
        └─────────┘ └──────┘│ └──────────────────┘
                              │
                       ┌─────▼─────┐
                       │tabApiClient│  HTTP/SSE → /v1/*
                       └───────────┘
```

### Core Modules (`clients/tabby-agent/src/`)

```
tabby-agent/src/
├── index.ts           # Entry point, lifecycle management
├── server.ts          # Agent server (IPC between IDE extension and core)
├── protocol.ts        # TypeScript-Server / IDE protocol definitions
├── codeCompletion/    # Code completion pipeline
│   ├── index.ts       # Pipeline orchestration
│   ├── buildRequest.ts# Build HTTP request from segments
│   ├── cache.ts       # Completion response caching
│   ├── contexts.ts    # Context collection (prefix/suffix/LSP)
│   ├── solution.ts    # Solution/segment processing
│   └── postprocess/   # 20+ result postprocessors
├── chat/              # Chat features
│   ├── index.ts       # Chat pipeline core
│   ├── inlineEdit.ts  # Inline edit (replace code in-place)
│   ├── smartApply.ts  # Smart apply (apply suggestion to any location)
│   ├── generateBranchName.ts  # Auto-generate git branch name
│   ├── generateCommitMessage.ts   # Generate commit message
│   └── prompts/       # Prompt templates for various tasks
├── contextProviders/  # Context source providers
│   ├── git.ts         # Git diff/blame context
│   ├── workspace.ts   # Workspace file context
│   └── document.ts    # Active document context
├── config/            # Configuration management
│   ├── index.ts       # Config loader + watcher
│   └── defaults.ts    # Default configuration values
└── http/              # HTTP client implementation
    ├── tabbyApiClient.ts  # REST/SSE client
    └── websocket.ts       # WebSocket client (chat streaming)
```

---

## 7.2 Completion Pipeline

代码补全的完整处理链路（IDE → Agent → Server → Agent → IDE）：

```
Step 1: Context Capture
┌─────────────────────────────────────┐
│ IDE provides:                       │
│   - segments.prefix (code before cursor)
│   - segments.suffix (code after cursor)
│   - language (file extension)       │
│   - relativeFilePath                │
│   - gitStatus (branch, untracked)   │
└──────────────┬──────────────────────┘

Step 2: Context Enhancement (Agent-side)
┌─────────────────────────────────────┐
│ tabby-agent adds context:           │
│   - LSP symbols near cursor          │
│   - recently modified lines           │
│   - imported module references        │
│   - git blame information             │
└──────────────┬──────────────────────┘

Step 3: Request Building
┌─────────────────────────────────────┐
│ tabApiClient builds HTTP POST:       │
│   POST /v1/completions               │
│   { language, segments, ... }         │
└──────────────┬──────────────────────┘

Step 4: Server Processing (crates/tabby)
┌─────────────────────────────────────┐
│ Server:                              │
│   - Prompt builder                   │
│   - Tantivy code search              │
│   - Model inference (streaming)       │
└──────────────┬──────────────────────┘

Step 5: Client Postprocessing (20+ rules)
┌─────────────────────────────────────┐
│ Results pass through postprocessors: │
│   ── sequence of filters & transforms ──
└──────────────┬──────────────────────┘

Step 6: Inline Display
┌─────────────────────────────────────┐
│ IDE shows ghost text suggestions      │
│ User: Tab → Accept / Ctrl+↓ → Cycle  │
└─────────────────────────────────────┘
```

### Postprocessors (20+)

Postprocessors 按固定顺序执行，每个可修改或过滤候选结果：

| # | Processor | 功能 |
|---|-----------|------|
| 1 | `trimSpace` | 修剪多余空白字符 |
| 2 | `limitScope` | 限制补全不超出当前作用域 |
| 3 | `normalizeIndentation` | 标准化缩进（空格→Tab 或反之） |
| 4 | `removeDuplicateSuffixLines` | 去除重复的尾行 |
| 5 | `dropMinimum` | 丢弃 token 数过少的结果 |
| 6 | `removeRepetitiveBlocks` | 去除连续重复代码块 |
| 7 | `deduplicate` | 去重多个候选（candidate deduplication） |
| 8 | `languageSpecificRules` | 语言特定规则 (Python 换行, JS semicolon) |
| 9 | `smartEnter` | 智能处理 Enter 键后的自动缩进 |
| 10-20 | ... | 其他语言/场景专用规则 |

### Caching

Agent 端对 completion 结果做短期缓存，避免重复请求相同的 context：

```typescript
// cache.ts — Completion cache key = hash(prefix + suffix + language)
const cache = new LruCache<string, Candidate[]>({ max: 100 });
```

---

## 7.3 Chat Features

tabby-agent 的 chat 模块支持多种交互模式：

| Feature | IDE Action | 调用 Server API |
|---------|-----------|-----------------|
| **Inline Edit** | Right-click → "Tabby: Inline Edit" | `/v1/chat/completions` + edit response parsing |
| **Smart Apply** | Select code → "Tabby: Smart Apply" | Chat → parse diff hunk → apply to editor |
| **Branch Name Gen** | Auto-trigger on git init | `/v1/chat/completions` with branch naming prompt |
| **Commit Message Gen** | `git commit` hook trigger | Chat with diff context |
| **Chat Panel** | Sidebar chat interface | SSE streaming via `/v1/chat/completions` |

### Prompt Templates (`chat/prompts/`)

| 模板文件 | 用途 |
|---------|------|
| `inline-edit.md` | "修改这段代码以..." |
| `fix-spelling.md` | "修复这段代码的语法错误" |
| `docs-generation.md` | "为这段代码生成文档字符串" |
| `smart-apply.md` | "根据用户需求生成代码并应用" |
| `branch-name.md` | "根据 diff 生成 git branch 名称" |
| `commit-msg.md` | "根据 diff 生成 commit message" |

---

## 7.4 Context Providers

Context Provider 收集 IDE 中的上下文信息，丰富发送给 server 的 request：

```typescript
// contextProviders/index.ts — Registry pattern
const providers: Record<string, ContextProvider> = {
  'git': GitContextProvider,      // Git diff / blame / branch info
  'workspace': WorkspaceContextProvider,  // Open files, recently accessed
  'document': DocumentContextProvider,     // Active document content
  'lsp': LSPContextProvider,           // Language server symbols
  'editor': EditorOptionsContextProvider,  // Tab size, indent style
};
```

---

## 7.5 Configuration System

### Config File Locations

| OS | Config Path | Data Path |
|----|------------|-----------|
| macOS | `~/.tabby-client/agent/config.toml` | `~/.tabby-client/agent/` |
| Linux | `~/.tabby-client/agent/config.toml` | `~/.tabby-client/agent/` |
| Windows | `%USERPROFILE%\.tabby-client\agent\config.toml` | `%USERPROFILE%\.tabby-client\agent\` |

### config.toml Structure (Agent)

```toml
# Agent 端配置 — IDE-specific overrides
server = { url = "http://localhost:8080" }

[telemetry]
enabled = true       # Allow usage event reporting to server

[autocomplete]
suggest_on_forward = true     # Forward cursor motion triggers suggestion
suggest_on_backward = false    # Backward does NOT trigger (reduces noise)

[chat]
model = "tabbyml/Qwen2-1.5B-Instruct"
```

### Environment Variables

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `TABBY_AGENT_SERVER_URL` | `http://localhost:8080` | Tabby server endpoint |
| `TABBY_AGENT_TOKEN` | (none) | User token for auth |
| `TABBY_AGENT_CONFIG_FILE` | auto-detect | Config file path override |

---

## 7.6 VS Code Extension

### Architecture

```
vscode/extension.ts          # Extension entry point (activate/deactivate)
    │
    ├── InlineCompletionProvider.ts   # vscode.InlineCompletionItemProvider impl
    ├── StatusBarItem.ts              # Status bar indicator
    ├── CodeActions.ts                # Quick fix / code actions
    ├── Config.ts                     # VS Code settings → agent config
    └── ...                           # Additional features (chat panel, etc.)
```

### Key Components

| 文件 | 职责 |
|------|------|
| `extension.ts` | `vscode.extensions.registerExtension()` — activates on workspace open |
| `InlineCompletionProvider.ts` | `provideInlineCompletionItems()` → calls agent for suggestions |
| `StatusBarItem.ts` | Shows connection status, model info in VS Code status bar |
| `CodeActions.ts` | Adds "Tabby: Explain This" / "Fix Code" code actions |

### VS Code Settings (`package.json` contributes)

```json
{
  "configuration": {
    "properties": {
      "tabby.server.url": {
        "type": "string",
        "default": "http://localhost:8080"
      },
      "tabby.autocomplete.enabled": {
        "type": "boolean",
        "default": true
      }
    }
  }
}
```

---

## 7.7 IntelliJ Plugin

### 技术栈

- **语言**: Java + Kotlin
- **构建**: Gradle (`build.gradle.kts`)
- **IDE Platform**: IntelliJ Platform SDK (compatible with 2022+)
- **Packaging**: Gradle `publishPlugin` task → `.zip` plugin package

### Plugin.xml Key Entries

```xml
<idea-plugin>
    <id>TabbyML.tabby</id>
    <name>Tabby</name>
    <depends>com.intellij.modules.platform</depends>
    <depends>com.intellij.modules.lang</depends>
    
    <!-- Inline completion -->
    <codeInsight.lineDataCollector>
        ...
    </codeInsight.lineDataCollector>
    
    <!-- Chat panel -->
    <toolWindow id="Tabby" icon="/icons/tabby.svg" ... />
    
    <!-- Status bar widget -->
    <statusBarWidgetFactory implementation="..."/>
</idea-plugin>
```

---

## 7.8 Vim/Neovim Client

### Installation

```bash
# Via plug.vim (Vim plugin manager)
Plug 'TabbyML/tabby-vim'

# Via Packer.nvim (Neovim)
use 'TabbyML/tabby-vim'
```

### Configuration (`~/.config/nvim/lua/tabby.lua`)

```lua
local tabby = require('tabby')
tabby.setup({
  server = { url = "http://localhost:8080" },
  completion = {
    enabled = true,
    auto_trigger = true,        -- Auto-trigger on idle
    keymap = {
      accept = "<Tab>",         -- Accept suggestion
      next = "<M-Tab>",          -- Cycle to next candidate
    }
  },
  chat = {
    enabled = true,
    toggle_keymap = "<C-space>",   -- Toggle chat panel
  }
})
```

---

## 7.9 Eclipse IDE Plugin

### Architecture

```
eclipse/
├── plugin.xml          # Eclipse plugin manifest
├── build.gradle        # Gradle build for Eclipse PDE target
└── src/
    ├── core/           # Tabby client core (Java port of agent logic)
    ├── completion/     -- Inline completion viewer / hover
    ├── chat/           -- Chat view (Eclipse IViewPart)
    └── preferences/    -- Preference page (server URL, token)
```

---

## 7.10 Protocol & Communication

### tabby-agent ↔ Server Protocol

| 协议 | 场景 | 传输方式 |
|------|------|---------|
| REST JSON | Completion / Health / Events | HTTP POST + JSON body |
| SSE (Server-Sent Events) | Chat streaming | EventSource API |
| WebSocket | Real-time features (EE) | Ws:// or Wss:// |

### Protocol Flow — Completion

```
IDE Extension
    │
    ├── Collect file context → build segments object
    ├── POST { language, segments } to /v1/completions
    ├── Receive streaming response
    ├── Postprocess candidates (20+ rules)
    └── Display inline suggestions to user

IDE Extension ← postprocessor results → User sees ghost text
```

### Tabby OpenAPI Package (`clients/tabby-openapi/`)

提供 TypeScript SDK：

```bash
npm install @tabbyml/tabby-openapi
```

```typescript
import { TabbyApiClient } from '@tabbyml/tabby-openapi';

const client = new TabbyApiClient({ serverUrl: 'http://localhost:8080' });
const health = await client.getHealth();
const completion = await client.postCompletions({
  language: 'python',
  segments: { prefix: 'def hello():\n  ', suffix: '' },
});
```
