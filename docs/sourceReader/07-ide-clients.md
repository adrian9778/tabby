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
                       ┌───────▼──────┐
                       │TabbyApiClient│  HTTP/SSE → /v1/*
                       └──────────────┘
```

### Core Modules (`clients/tabby-agent/src/`)

tabby-agent 的 `index.ts` 只做一件事：实例化 `clients/tabby-agent/src/index.ts` · `Server` 并调用 `server.listen()`，进程以 **LSP server** 形态运行（`server.ts` · `Server` `+0`，内部用 `vscode-languageserver` 的 `createConnection(ProposedFeatures.all)`），各 Feature/Provider 都在 `Server` 里装配。

```
tabby-agent/src/
├── index.ts              # 入口：new Server(); server.listen()
├── server.ts             # LSP Server，装配/注册各 Feature 与 ContextProvider
├── protocol.ts           # 扩展 vscode-languageserver 协议（inline completion 等）
├── env.ts feature.ts status.ts command.ts telemetry.ts
├── codeLens.ts codeSearch.ts certsLoader.ts
├── codeCompletion/       # 代码补全流水线（目录）
│   ├── index.ts          # CompletionProvider：流水线编排
│   ├── buildRequest.ts   # 由 segments 构造 HTTP 请求
│   ├── contexts.ts       # 上下文采集（prefix/suffix 等）
│   ├── cache.ts          # CompletionCache（LRUCache 子类）
│   ├── solution.ts       # CompletionSolution / 候选合并
│   ├── debouncer.ts latencyTracker.ts statistics.ts
│   └── postprocess/      # 结果后处理（preCacheProcess + postCacheProcess）
├── chat/                 # Chat 能力（目录）
│   ├── index.ts          # ChatFeature
│   ├── inlineEdit.ts     # ChatEditProvider（就地编辑）
│   ├── smartApply.ts     # SmartApplyFeature
│   ├── smartRange.ts     # getSmartApplyRange / fuzzyApplyRange
│   ├── generateBranchName.ts generateCommitMessage.ts
│   ├── global.ts utils.ts
│   └── prompts/          # 提示词模板（*.md，详见 7.3）
├── contextProviders/     # 上下文来源（均为独立文件/目录，见 7.4）
│   ├── workspace.ts      # WorkspaceContextProvider
│   ├── documentContexts.ts # TextDocumentReader
│   ├── declarationSnippets.ts # DeclarationSnippetsProvider
│   ├── recentlyChangedCodeSearch.ts # RecentlyChangedCodeSearch
│   ├── editorOptions.ts  # EditorOptionsProvider
│   ├── editorVisibleRanges.ts # EditorVisibleRangesTracker
│   └── git/              # GitContextProvider —— 是目录，单文件 git.ts 不存在
│       ├── index.ts
│       └── gitCommand.ts
├── config/               # 配置管理（目录）
│   ├── index.ts          # Configurations：分层合并 + 变更事件
│   ├── configFile.ts     # ConfigFile：读 config.toml + chokidar watch
│   ├── default.ts        # defaultConfigData 默认值（单数；复数 defaults.ts 不存在）
│   └── type.d.ts
├── http/                 # 与 server 通信（目录）
│   ├── tabbyApiClient.ts # REST 客户端（/v1/*）
│   ├── stream.ts         # readChatStream：SSE 流解析
│   ├── proxy.ts
│   └── cloudApi.d.ts
├── dataStore/            # DataStore：本地持久化（目录）
├── extensions/           # textDocuments.ts 等扩展支持（目录）
├── logger/               # getLogger / fileLogger（目录）
├── syntax/               # parser.ts：tree-sitter 语法解析（目录）
└── utils/                # array/diff/range/string/types 等（目录）
```

> 注：原文树里的 `contextProviders/git.ts`、`contextProviders/document.ts`、`config/defaults.ts`、`http/websocket.ts` 均不存在。git 是目录 `contextProviders/git/`；文档读取在 `documentContexts.ts`；默认配置是 `config/default.ts`（单数）；聊天流式走 `http/stream.ts`，agent 全仓**没有任何 WebSocket 代码**（`grep -rni websocket clients/tabby-agent/src/` 无命中）。7.1 架构图 Providers 框里的 `ws` 指 `workspace.ts`（`WorkspaceContextProvider`），不是 WebSocket；Tabby 里的 WebSocket 只在 server 侧 Hub 通道（`ee/tabby-webserver/src/axum/websocket.rs`、`ee/tabby-webserver/src/hub.rs`），与 IDE 客户端无关。

---

## 7.2 Completion Pipeline

代码补全的完整处理链路（IDE → Agent → Server → Agent → IDE）：

```
Step 1: Context Capture
┌─────────────────────────────────────┐
│ IDE (LSP client) sends:             │
│   - textDocument (uri + version)    │
│   - position                        │
│   - context.selectedCompletionInfo  │
│   - editorOptions (tabSize 等)      │
└──────────────┬──────────────────────┘

Step 2: Context Enhancement (Agent-side)
┌─────────────────────────────────────┐
│ tabby-agent builds the context:     │
│   - prefix / suffix（按光标裁剪）    │
│   - declaration snippets             │
│   - snippets from changed files      │
│     (本地索引，非 git diff)          │
│   - snippets from recently opened    │
│     files                            │
│   - clipboard                        │
│   - git repository root + remote url │
└──────────────┬──────────────────────┘

Step 3: Request Building
┌─────────────────────────────────────┐
│ TabbyApiClient builds HTTP POST:     │
│   POST /v1/completions               │
│   { language, segments }             │
│   segments: prefix/suffix/filepath/  │
│   git_url/declarations/…             │
└──────────────┬──────────────────────┘

Step 4: Server Processing (crates/tabby)
┌─────────────────────────────────────┐
│ Server:                              │
│   - Prompt builder                   │
│   - Tantivy code search              │
│   - Model inference（completion 非流式）│
└──────────────┬──────────────────────┘

Step 5: Client Postprocessing
┌─────────────────────────────────────┐
│ Results pass through postprocessors: │
│   preCacheProcess → cache → postCacheProcess │
└──────────────┬──────────────────────┘

Step 6: Inline Display
┌─────────────────────────────────────┐
│ IDE shows ghost text suggestions      │
│ VS Code 默认：Tab 接受整条 / Ctrl+→  │
│ 逐词接受 / Alt+\ 手动触发            │
└─────────────────────────────────────┘
```

### Postprocessors

后处理在 `clients/tabby-agent/src/codeCompletion/postprocess/index.ts` 里分两段执行：写缓存前 `preCacheProcess` `+0`，取缓存后 `postCacheProcess` `+0`。共 **12 个**后处理算子（原文「20+」不实），每个算子一个同名文件：

| # | Processor | 阶段 | 功能 |
|---|-----------|------|------|
| 1 | `trimMultiLineInSingleLineMode` | pre | 单行模式下裁掉多余换行 |
| 2 | `removeLineEndsWithRepetition` | pre | 去除行尾重复片段 |
| 3 | `dropDuplicated` | pre/post | 丢弃重复候选 |
| 4 | `trimSpace` | pre/post | 修剪多余空白 |
| 5 | `dropMinimum` | pre/post | 丢弃过短结果（`minCompletionChars`） |
| 6 | `removeRepetitiveBlocks` | post | 去除连续重复代码块 |
| 7 | `removeRepetitiveLines` | post | 去除连续重复行 |
| 8 | `limitScope` | post | 限制补全不超出当前作用域 |
| 9 | `removeDuplicatedBlockClosingLine` | post | 去除重复的块闭合行 |
| 10 | `formatIndentation` | post | 规整缩进格式 |
| 11 | `normalizeIndentation` | post | 标准化缩进风格 |
| 12 | `removeDuplicateSuffixLines` | post | 去除与 suffix 重复的尾行 |

> 原文表中的 `deduplicate`、`languageSpecificRules`、`smartEnter` 等条目在 `postprocess/` 目录中不存在；`postprocess/` 另有 `base.ts`（`PostprocessFilter` 类型）、`limitScopeByIndentation.ts`（被 `limitScope` 复用）与 `golden/`（黄金测试用例）。

### Caching

Agent 端对 completion 结果做短期缓存，类定义在 `clients/tabby-agent/src/codeCompletion/cache.ts` · `CompletionCache` `+0`（直接继承 `LRUCache<string, CompletionSolution>`，默认 `max = 100`、`ttl = 5 分钟`）。缓存 key 由 `calculateCompletionContextHash` `+0` 计算：对「当前文档的 uri + prefix + suffix」以及「其它已打开文档的 uri + version」做 `object-hash`（并非原文所说的 prefix+suffix+language）：

```typescript
// cache.ts · CompletionCache: 继承 LRUCache，max=100 / ttl=5min
// calculateCompletionContextHash(context, textDocuments):
//   hash({ document: {uri, prefix, suffix}, otherDocuments: [{uri, version}] })
```

---

## 7.3 Chat Features

tabby-agent 的 chat 能力（`clients/tabby-agent/src/chat/`）支持多种交互模式，统一经 `http/tabbyApiClient.ts` · `TabbyApiClient` `+0` 打到 `/v1/chat/completions`（`index.ts` · `ChatFeature` `+0`）：

| Feature | 实现（chat/） | 调用 Server API |
|---------|-----------|-----------------|
| **Inline Edit** | `inlineEdit.ts` · `ChatEditProvider` `+0` | `/v1/chat/completions` + edit response 解析 |
| **Smart Apply** | `smartApply.ts` · `SmartApplyFeature` `+0`（+ `smartRange.ts` 定位区间） | Chat → 解析生成的代码 → 应用到编辑器 |
| **Branch Name Gen** | `generateBranchName.ts` · `BranchNameGenerator` `+0`，由 IDE 命令触发（如 VS Code `tabby.chat.generateBranchName`） | `/v1/chat/completions` + branch 命名 prompt + git diff |
| **Commit Message Gen** | `generateCommitMessage.ts` · `CommitMessageGenerator` `+0`，由 IDE 命令触发 | Chat + diff 上下文 |
| **Chat Panel** | 侧栏聊天面板（`tabby-chat-panel`） | SSE 流式：`/v1/chat/completions`，由 `http/stream.ts` · `readChatStream` `+0` 解析 |

### Prompt Templates (`chat/prompts/`)

`chat/prompts/` 下共 10 个 `.md` 模板 + 1 个 `index.d.ts`（让 TS 能 `import` md）。这些模板不是运行时读文件，而是在 `clients/tabby-agent/src/config/default.ts` 顶部 `import` 后塞进对应配置项的 `promptTemplate`（见 default.ts 的 import 清单）。原文六个名字全部不存在，真实对应关系：

| 模板文件 | 绑定配置项 | 用途 |
|---------|-----------|------|
| `edit-command-insert.md` | `chat.edit.promptTemplate.insert` | 插入模式就地编辑（inline-edit 之一） |
| `edit-command-replace.md` | `chat.edit.promptTemplate.replace` | 替换模式就地编辑（inline-edit 之一） |
| `fix-spelling-and-grammar.md` | `chat.edit.presetCommands["/fix"]` | 修正拼写/语法（纯文本/Markdown） |
| `generate-docs.md` | `chat.edit.presetCommands["/doc"]` | 为选中代码生成文档字符串 |
| `generate-smart-apply.md` | `chat.smartApply.promptTemplate` | Smart Apply 生成落地代码 |
| `provide-smart-apply-line-range.md` | `chat.smartApplyLineRange.promptTemplate` | 判定 smart apply 的插入行区间 |
| `generate-branch-name.md` | `chat.generateBranchName.promptTemplate` | 根据 diff 生成 git branch 名称 |
| `generate-commit-message.md` | `chat.generateCommitMessage.promptTemplate` | 根据 diff 生成 commit message |
| `include-file-context-list.md` | `chat.edit.fileContext.promptTemplate[0]` | 渲染文件上下文列表 |
| `include-file-context-item.md` | `chat.edit.fileContext.promptTemplate[1]` | 渲染单个文件上下文条目 |

> 原文写的 `inline-edit.md`/`fix-spelling.md`/`docs-generation.md`/`smart-apply.md`/`branch-name.md`/`commit-msg.md` 均不存在；`ls clients/tabby-agent/src/chat/prompts/` 得到上面 11 个文件。

---

## 7.4 Context Providers

Context Provider 收集 IDE 中的上下文信息，丰富发送给 server 的 request。

注册并不在某个 `contextProviders/index.ts`（该文件不存在），而是在 `clients/tabby-agent/src/server.ts` · `Server` `+0` 里逐个 `new` 出 provider 实例、注入 `CompletionProvider`，并加入 `featureComponents` 数组统一初始化（provider 字段声明在 `Server` `+16`~`+22`，注入 `CompletionProvider` 在 `+25`，`featureComponents` 数组在 `+61`）：

```typescript
// server.ts · Server —— provider 字段（节选自 ls + 读 server.ts 得到的真实类名）
this.textDocumentReader        // contextProviders/documentContexts.ts 的 TextDocumentReader
this.workspaceContextProvider  // WorkspaceContextProvider   (workspace.ts)
this.gitContextProvider        // GitContextProvider          (git/index.ts)
this.declarationSnippetsProvider // DeclarationSnippetsProvider (declarationSnippets.ts)
this.recentlyChangedCodeSearch   // RecentlyChangedCodeSearch   (recentlyChangedCodeSearch.ts)
this.editorVisibleRangesTracker  // EditorVisibleRangesTracker  (editorVisibleRanges.ts)
this.editorOptionsProvider       // EditorOptionsProvider       (editorOptions.ts)
```

> `contextProviders/` 里**没有** `index.ts`，也没有名为 `DocumentContextProvider` / `LSPContextProvider` 的类。git provider 是目录 `contextProviders/git/`（`index.ts` 提供 `GitContextProvider`，只取「仓库根路径 + `git diff`」，无 blame）；文档内容读取在 `documentContexts.ts` 的 `TextDocumentReader`；没有独立的 "lsp" provider——符号/声明类上下文由 `declarationSnippets.ts` 承担。

---

## 7.5 Configuration System

### Config File Locations

| OS | Config Path | Data Path |
|----|------------|-----------|
| macOS | `~/.tabby-client/agent/config.toml` | `~/.tabby-client/agent/` |
| Linux | `~/.tabby-client/agent/config.toml` | `~/.tabby-client/agent/` |
| Windows | `%USERPROFILE%\.tabby-client\agent\config.toml` | `%USERPROFILE%\.tabby-client\agent\` |

路径由 `clients/tabby-agent/src/config/configFile.ts` · `getConfigFile` `+0` 用 `os.homedir()` 拼出（三个 OS 同一条规则，Windows 的 homedir 即 `%USERPROFILE%`；浏览器形态下该函数直接返回 `undefined`）；同目录下的 `~/.tabby-client/agent/data.json` 是 DataStore 落盘位置（`dataStore/dataFile.ts` · `getFileDataStore` `+0`），`~/.tabby-client/agent/logs/` 是轮转日志目录（`clients/tabby-agent/src/logger/fileLogger.ts`，保留 30 天）——两者都是运行期生成、不在仓库里。

### config.toml Structure (Agent)

下面的键名全部取自 `clients/tabby-agent/src/config/default.ts` · `defaultConfigData` `+0` 的真实字段（这是一份**最小示例**，不是配置文件模板清单）：

```toml
# 示例：~/.tabby-client/agent/config.toml
[server]
endpoint = "http://localhost:8080"   # 默认值，注意键名是 endpoint 而非 url
token = ""

[completion.debounce]
mode = "adaptive"
interval = 250

[postprocess]
minCompletionChars = 4

[anonymousUsageTracking]
disable = false                       # 匿名使用统计开关（默认 false = 允许上报；VS Code 的 tabby.config.telemetry 映射到这里）

[logs]
level = "silent"
```

> 原文的 `server.url`、`[telemetry] enabled`、`[autocomplete] suggest_on_forward/suggest_on_backward`、`[chat] model` 四组键在 `default.ts` / `config/type.d.ts` 里都不存在：agent 配置树只有 `server / proxy / completion / postprocess / chat / logs / tls / anonymousUsageTracking` 八个顶层键；补全触发方式由 IDE 侧设置（如 VS Code 的 `tabby.settings.advanced["inlineCompletion.triggerMode"]`）决定，chat 使用的模型由 server 侧选定，agent 配置里没有 `model` 键。

### Environment Variables

**tabby-agent 不读取任何 `TABBY_AGENT_*` 环境变量**（原文表格里的 `TABBY_AGENT_SERVER_URL` / `TABBY_AGENT_TOKEN` / `TABBY_AGENT_CONFIG_FILE` 均不存在）。实测 `grep -rn process.env clients/tabby-agent/src/` 只命中 `clients/tabby-agent/src/env.ts` 的两个内部测试开关 `IS_BROWSER`、`IS_TEST`。配置的实际来源是四层合并（`config/index.ts` · `mergeConfig` `+0`，优先级由低到高）：

| 层 | 来源 | 能覆盖的键 |
|----|------|-----------|
| default | `config/default.ts` · `defaultConfigData` | — |
| configFile | `~/.tabby-client/agent/config.toml`（chokidar watch） | 全部 |
| clientProvided | IDE 扩展经 LSP `ClientProvidedConfig` 传入 | 仅 `server` / `proxy` / `anonymousUsageTracking` |
| serverProvided | server 下发并存在 dataStore | 仅 `anonymousUsageTracking.disable`（由 `disable_client_side_telemetry` 映射） |

若确实要用命令行/宿主变量控制连接对象，只有 IDE 客户端自己的入口——例如 vim 的 Vim 全局变量 `g:tabby_agent_start_command`（默认 `['npx','tabby-agent','--stdio']`，见 `clients/vim/autoload/tabby/lsp.vim`），它是「启动 agent 的命令」而不是 agent 的配置项。

---

## 7.6 VS Code Extension

### Architecture

```
clients/vscode/src/extension.ts     # Extension entry point (activate/deactivate)
    │
    ├── lsp/client.ts               # 与 tabby-agent 的 LSP 连接
    ├── InlineCompletionProvider.ts # vscode.InlineCompletionItemProvider impl
    ├── StatusBarItem.ts            # Status bar indicator
    ├── CodeActions.ts              # Quick fix / code actions
    ├── Config.ts                   # VS Code settings → agent config
    ├── ContextVariables.ts git/ commands/ chat/ inline-edit/ code-action/
    └── ...                         # Additional features (chat panel, etc.)
```

### Key Components

| 文件 | 职责 |
|------|------|
| `extension.ts` | 导出 `activate` `+0` / `deactivate` `+0`，在 `activate` 里依次装配 `createClient`、`Config`、`ContextVariables`、`InlineCompletionProvider`、`GitProvider`、`ChatSidePanelProvider`（webview id `tabby.chatView`）、`StatusBarItem`、`Commands`、`KeyBindingManager`、`CodeActions`；`clients/vscode/package.json` 的 `activationEvents` 是 `onStartupFinished`（不是「打开工作区时」，VS Code 也已不推荐声明 `workspaceContains`） |
| `InlineCompletionProvider.ts` | `InlineCompletionProvider` `+0` 实现 `vscode.InlineCompletionItemProvider`，`provideInlineCompletionItems` `+0` 经 `lsp/client.ts` 向 agent 请求 |
| `StatusBarItem.ts` | `StatusBarItem` `+0`：显示连接状态 / 需要处理的问题（模型信息由 server 侧提供） |
| `CodeActions.ts` | `CodeActions` `+0` 注册两个 provider：`code-action/InlineEdit.ts` · `InlineEditCodeActionProvider`（"Edit using Tabby"）与 `code-action/QuickFix.ts` · `QuickFixCodeActionProvider`（"Fix using Tabby" / "Explain using Tabby"） |
| `Config.ts` | `Config` `+0` 监听 `tabby` / `editor.inlineSuggest` / `http.proxy` 等设置变化，用 `buildClientProvidedConfig` `+0` 生成 `ClientProvidedConfig` 传给 agent |

### VS Code Settings (`package.json` contributes)

`clients/vscode/package.json` 的 `contributes.configuration` 实际只有四个顶层键（原文的 `tabby.server.url`、`tabby.autocomplete.enabled` 不存在）：

```json
{
  "configuration": {
    "properties": {
      "tabby.endpoint": {
        "type": "string",
        "default": "http://localhost:8080"
      },
      "tabby.keybindings": {
        "type": "string",
        "enum": ["vscode-style", "tabby-style"],
        "default": "vscode-style"
      },
      "tabby.config.telemetry": {
        "type": "boolean",
        "default": false
      },
      "tabby.settings.advanced": {
        "default": {},
        "properties": {
          "inlineCompletion.triggerMode": { "enum": ["automatic", "manual"], "default": "automatic" },
          "inlineCompletion.disabledLanguages": { "type": "array", "default": [] },
          "chatEdit.history": { "type": "integer", "default": 20 }
        }
      }
    }
  }
}
```

> 补全开关没有单独的 `enabled` 布尔项：整体启停由 VS Code 自带的 `editor.inlineSuggest.enabled` 控制（`Config.ts` · `vscodeInlineSuggestEnabled` `+0` 读它），按语言禁用走 `tabby.settings.advanced["inlineCompletion.disabledLanguages"]`。`Config.ts` · `AdvancedSettings` 接口里还多了一个未写进 `package.json` 的 `useVSCodeProxy` 键。

---

## 7.7 IntelliJ Plugin

### 技术栈

- **语言**: Kotlin（实测 `clients/intellij/src/main/kotlin/` 下 91 个 `.kt`、0 个 `.java`——原文写「Java + Kotlin」不实）
- **构建**: Gradle Kotlin DSL（`clients/intellij/build.gradle.kts`，插件 `org.jetbrains.intellij.platform` 2.0.0 + Kotlin 1.9.25，`jvmTarget = "17"`）
- **IDE Platform**: IntelliJ Platform SDK，`sinceBuild.set("231")`（即 2023.1 起，非原文的 2022+），`untilBuild` 置空不设上限
- **Packaging**: Gradle `publishPlugin` task → `.zip` plugin package（`.github/workflows/release-intellij.yml` 以 `arguments: publishPlugin` 调用，tag 形如 `intellij@x.y.z` 走 stable 渠道，否则 alpha）

运行时形态与 VS Code 一致：插件本身是 **LSP 客户端**，由 `clients/intellij/src/main/kotlin/com/tabbyml/intellijtabby/lsp/ConnectionService.kt` 用 node 二进制把随插件打包的 agent 入口（`pluginPath.resolve(…)` 得到的 `…/tabby-agent/node/index.js`，运行期路径、不在仓库源码树里）拉起为子进程并走 stdio，因此 plugin.xml 的描述里要求 Node.js v18+。

### Plugin.xml Key Entries

`clients/intellij/src/main/resources/META-INF/plugin.xml`（原文的 `<id>TabbyML.tabby</id>`、`com.intellij.modules.lang`、`codeInsight.lineDataCollector` 三处都不存在）：

```xml
<idea-plugin>
    <id>com.tabbyml.intellij-tabby</id>
    <name>Tabby</name>
    <vendor url="https://tabbyml.com">TabbyML</vendor>

    <depends>com.intellij.modules.platform</depends>
    <depends optional="true" config-file="plugin-Git4Idea.xml">Git4Idea</depends>
    <depends optional="true" config-file="plugin-kotlin.xml">org.jetbrains.kotlin</depends>

    <extensionPoints>
        <extensionPoint name="languageSupportProvider"
                        interface="com.tabbyml.intellijtabby.languageSupport.LanguageSupportProvider"
                        dynamic="true"/>
    </extensionPoints>

    <extensions defaultExtensionNs="com.intellij">
        <!-- Inline completion / lifecycle -->
        <postStartupActivity implementation="...events.StartupActivity"/>
        <editorFactoryListener implementation="...events.EditorFactoryListener"/>
        <actionPromoter order="last" implementation="...actionPromoter.EditorActionPromoter"/>
        <statusBarWidgetFactory id="com.tabbyml.intellijtabby.status.StatusBarWidgetFactory"
                                implementation="...widgets.StatusBarWidgetFactory"/>
        <projectService serviceInterface="...git.GitProvider"
                        serviceImplementation="...git.DummyGitProvider"/>

        <!-- Chat panel -->
        <toolWindow id="Tabby" anchor="right" icon="com.tabbyml.intellijtabby.Icons.Chat"
                    factoryClass="com.tabbyml.intellijtabby.widgets.ChatToolWindowFactory"/>

        <!-- Inline chat：代码透镜 + 意图 + 高亮，不是 lineDataCollector -->
        <intentionAction>
            <className>...inlineChat.InlineChatIntentionAction</className>
        </intentionAction>
        <codeInsight.codeVisionProvider implementation="...inlineChat.InlineChatLoadingCodeVisionProvider"/>
        <!-- 另有 Cancel / Accept / Discard 三个同类 codeVisionProvider -->
        <highlightingPassFactory implementation="...inlineChat.DiffHighlighterRegister"/>
        <projectConfigurable instance="...settings.Configurable" displayName="Tabby" parentId="editor"/>
    </extensions>
</idea-plugin>
```

> 省略号 `...` 代表包名前缀 `com.tabbyml.intellijtabby`；`notificationGroup`（info / warning）也在此文件中声明。

---

## 7.8 Vim/Neovim Client

### Installation

发布用的仓库是 `TabbyML/vim-tabby`（由主仓库 `clients/vim/` 同步而来，见 `clients/vim/README.md` 结尾与 `.github/workflows/release-vim.yml`）——原文写的 `TabbyML/tabby-vim` 不存在：

```bash
# Via plug.vim (Vim plugin manager)
Plug 'TabbyML/vim-tabby'

# Via Packer.nvim (Neovim)
use 'TabbyML/vim-tabby'
```

2.0 之后插件只做两件事：**LSP client 扩展**（`textDocument/inlineCompletion` 等）+ **ghost text UI**；tabby-agent 不再是内置脚本，要先 `npm install --global tabby-agent`（Node.js v18+），由 LSP client 用 `npx tabby-agent --stdio` 拉起。受支持的 LSP client 目前只有 Neovim 内置 LSP + `nvim-lspconfig`（`autoload/tabby/lsp/nvim_lsp.vim`），其余标注「in development」；渲染要求 Neovim 或带 `+textprop` 的 Vim 9.0+。

### Configuration (`~/.config/nvim/lua/tabby.lua`)

`require('tabby')` 只返回 `{ lsp = require 'tabby.lsp' }`，**没有 `setup()` 函数**（原文那段 `tabby.setup({ server = {...}, completion = {...}, chat = {...} })` 是假想 API）。可配置项是初始化前设置的 Vim 全局变量，取自 `clients/vim/README.md` 的配置表与代码默认值：

```lua
-- ~/.config/nvim/init.lua：变量在 plugin 初始化前生效
vim.g.tabby_agent_start_command = {"npx", "tabby-agent", "--stdio"}
vim.g.tabby_inline_completion_trigger = "auto"          -- "auto" | "manual"
vim.g.tabby_inline_completion_keybinding_accept = "<Tab>"
vim.g.tabby_inline_completion_keybinding_trigger_or_dismiss = "<C-\\>"
vim.g.tabby_inline_completion_insertion_leading_key = "<C-R><C-O>="

require("lazy").setup({
  { "TabbyML/vim-tabby", lazy = false, dependencies = { "neovim/nvim-lspconfig" } },
})
```

对应的定义处：`clients/vim/autoload/tabby/lsp.vim`（`g:tabby_agent_start_command`、`g:tabby_lsp_client`）、`clients/vim/autoload/tabby/inline_completion/service.vim`（trigger / insertion leading key）、`clients/vim/autoload/tabby/inline_completion/keybindings.vim`（两个 keybinding）。

> server 地址与 token 不在 Vim 变量里配，而是写在 agent 自己的 `~/.tabby-client/agent/config.toml`（`[server] endpoint` / `token`，见 7.5）。vim 客户端**没有 chat 面板**：`clients/vim/` 下只有 `lsp` 与 `inline_completion` 两套实现，README 亦无 Chat 章节。

---

## 7.9 Eclipse IDE Plugin

### Architecture

```
clients/eclipse/
├── package.json                    # name: tabby4eclipse；build = node scripts/copy-dependencies.js
├── scripts/copy-dependencies.js    # 把 pnpm 构建出的 tabby-agent / tabby-chat-panel 拷进插件目录
├── plugin/                         # PDE 插件工程（Eclipse 里 Import 的根目录）
│   ├── plugin.xml                  # Eclipse plugin manifest（扩展点声明）
│   ├── META-INF/MANIFEST.MF  build.properties
│   ├── images/  chat-panel/index.html   # 聊天面板 webview 资源
│   └── src/com/tabbyml/tabby4eclipse/
│       ├── Activator.java  Startup.java  Version.java  Logger.java …
│       ├── lsp/                     # ConnectionProvider, LanguageClientImpl, LanguageServerService
│       │   └── protocol/            # ILanguageServer / ClientProvidedConfig / GitDiffParams …
│       ├── inlineCompletion/        # service + renderer/（ghost text 绘制）+ trigger/
│       ├── chat/                    # ChatView.java 等（org.eclipse.ui.views 贡献的 IViewPart）
│       ├── commands/                # chat/ 与 inlineCompletion/ 的 handler
│       ├── git/  editor/  preferences/  statusbar/
├── feature/                        # feature.xml（id="com.tabbyml.features.tabby4eclipse"）+ build.properties
└── docs/                           # README 用的截图
```

> 原文这棵树有三处不实：`clients/eclipse/` 下**没有** `build.gradle`（构建走 Eclipse PDE + pnpm/turbo，见 `clients/eclipse/README.md` 第 3 步）；`plugin.xml` 在 `plugin/` 子目录而非顶层；Java 侧**不是**「agent 逻辑的 Java 移植」——`plugin.xml` 里声明的是 `org.eclipse.lsp4e.languageServer`（server class = `lsp.ConnectionProvider`，clientImpl = `lsp.LanguageClientImpl`，serverInterface = `lsp.protocol.ILanguageServer`），即一个 lsp4e 语言客户端，补全/chat 逻辑仍在被 node 拉起的 tabby-agent 里（`lsp/ConnectionProvider.java` 从偏好页取 node 可执行路径）。`plugin/tabby-agent/` 目录是构建产物，仓库里不存在。

---

## 7.10 Protocol & Communication

### tabby-agent ↔ Server Protocol

| 协议 | 场景 | 传输方式 |
|------|------|---------|
| REST JSON | Completion `/v1/completions`、Health `/v1/health`、Events `/v1/events`、ServerSetting `/v1beta/server_setting` | HTTP POST + JSON body；由 `http/tabbyApiClient.ts` · `TabbyApiClient` `+0` 用 `openapi-fetch` 的 `createClient` 发起，类型来自 `tabby-openapi/compatible` |
| SSE (Server-Sent Events) | Chat streaming（`/v1/chat/completions`，旧 server 兜底 `/v1beta/chat/completions`） | **不是** 浏览器 `EventSource`：`fetch` 拿到 `text/event-stream` 响应体后，`http/stream.ts` · `readChatStream` `+0` 用 `eventsource-parser` 的 `EventSourceParserStream` 逐块解析（`EventSource` 无法带 POST body 与自定义鉴权头） |
| LSP（JSON-RPC over stdio / IPC） | IDE 客户端 ↔ tabby-agent 之间（**不是** ↔ server） | `server.ts` · `Server` `+0` 用 `vscode-languageserver` 的 `createConnection(ProposedFeatures.all)`；浏览器形态走 `BrowserMessageReader/Writer` |

> 原文表格里的 WebSocket 一行不成立。IDE 客户端与 server 之间不存在 WebSocket 通道：`clients/tabby-agent/src`、`clients/vscode/src`、`clients/intellij/src`、`clients/eclipse/plugin/src`、`clients/vim` 五处 `grep -rni websocket` 全部无命中。`clients/` 里唯一含 WebSocket 代码的是通用 RPC 库 `clients/tabby-threads/source/targets/web-socket-browser.ts` · `createThreadFromBrowserWebSocket`——它只是与 iframe / message-port / broadcast-channel / web-worker 并列的一个**可选 transport，且没有任何 IDE 客户端调用它**（chat 面板实际用 iframe 传输：`clients/tabby-chat-panel/src/browser.ts` · `createThreadFromIframe`、`clients/vscode/src/chat/createClient.ts` · `createThreadFromWebview`）。Tabby 里真正被使用的 WebSocket 只有 server 侧 Hub——`ee/tabby-webserver/src/axum/websocket.rs` 与 `ee/tabby-webserver/src/hub.rs`（`WebSocketTransport`），用于自托管 tabby 实例反向连接到 webserver，与本章的 IDE 客户端无关。

### Protocol Flow — Completion

```
IDE Extension  --LSP/JSON-RPC-->  tabby-agent  --HTTP-->  Server
    │
    ├── Collect file context → build segments object
    ├── POST { language, segments } to /v1/completions
    ├── Receive JSON response（completion 非流式，整条返回）
    ├── Postprocess candidates (12 rules, 见 7.2)
    └── Display inline suggestions to user
```

### Tabby OpenAPI Package (`clients/tabby-openapi/`)

它是一个**纯类型包**（`package.json` 里 `files` 只收 `lib/**/*.d.ts` 与 `compatible/**/*.d.ts`，`types` 指向 `lib/index.d.ts`，description 为 “Tabby API schema for typescript”），包名是 `tabby-openapi`（**没有** `@tabbyml/` scope，也不提供运行时 SDK——原文的 `new TabbyApiClient({ serverUrl })` / `client.getHealth()` / `client.postCompletions()` 三个 API 都不存在）。类型由 `clients/tabby-openapi/scripts/codegen.js` 用 `openapi-typescript` 从 `clients/tabby-openapi/openapi.json` 生成（`pnpm codegen:fetch` 会先从 `http://127.0.0.1:8080/api-docs/openapi.json` 拉最新 schema），`compatible/index.d.ts` 再叠上对 server 0.2.x（`/v1/health`）与 0.10.x（`/v1beta/chat/completions`）的向后兼容。

真实用法即 tabby-agent 自己的用法（`clients/tabby-agent/src/http/tabbyApiClient.ts` 顶部）：

```typescript
// 只有类型，HTTP 调用由 openapi-fetch 负责
import type { paths as TabbyApi, components as TabbyApiComponents } from "tabby-openapi/compatible";
import createClient from "openapi-fetch";

const api = createClient<TabbyApi>({ baseUrl: endpoint, headers: { Authorization: auth } });
const { data } = await api.POST("/v1/completions", {
  body: {
    language: "python",
    segments: { prefix: "def hello():\n  ", suffix: "" },
  },
});
```
