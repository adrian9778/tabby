# sourceReader 文档库校对规范（UPDATE-SPEC）

> 基准提交：`main` @ `f05db40e`（2026-09-02），workspace 版本 `0.33.0-dev.0`
> 本轮任务日期：2026-10-15
> **代码侧零漂移**：`21b29048`（2026-06-30）之后没有任何提交触碰 `crates/`、`ee/`、`clients/`、`python/`、`Cargo.toml`

## 0. 本轮性质（先读这段）

**不是行号刷新，是断言真伪校对。** 全库 330 处路径引用里 **45 处指向根本不存在的文件**，另有 7 处只是省略了前缀。误差形态是「作者按心智模型臆造文件名」，因此**改引用时必须连带核对它所在的整句/整表/整段描述**——路径错往往意味着那句话本身也错。

校验工具：`python3 docs/sourceReader/.progress/sr_tools.py <cmd>`

- `verify docs/sourceReader/NN-xxx.md` — 只查**文件路径能否解析**，分 5 档：`suspect` / `negated` / `placeholder` / `runtime-config` / `bare-name` / `shortened`；只有 `suspect` 要处理
- `locate SYM` — 全库找符号定义；`outline FILE` — 文件符号骨架；`offset FILE SYM` — 定义行号
- 已排除 vendored `crates/llama-cpp-server/llama.cpp`（10 万行第三方代码，不过滤会被同名符号淹没）

## 1. 定位记法

正文一律「**文件 · 符号 `+N`**」，`+0` = 定义行，`+N` = 函数体内第 N 行。**禁止裸行号**（`file.rs:123` 形式只允许出现在文末源码索引）。

| 场景 | 写法 |
|---|---|
| 函数/方法 | `crates/tabby-common/src/config.rs` · `Config::load` `+25` |
| 类型 | `...` · `enum ModelConfig` `+0` |
| 模块级常量/静态 | `Module:crates/tabby-common/src/path.rs` · `TABBY_ROOT` |
| 纯目录结构（树状图） | 照实写目录树，不进正文锚点 |

## 2. 内容红线

1. 严禁编造文件名、配置项、API、默认值、数量。**任何数字（文件数/行数/迁移数/特性数）必须 `ls`/`wc` 实测后写入**，不能沿用原文。
2. 示例/占位路径必须显式标注「示例」，例如 `// 新增：crates/http-api-bindings/src/completion/my_provider.rs（示例文件名）`，不得写成暗示仓库已有该文件的形式。
3. 禁止「同上/略」替代核心内容；保留各篇原有的章节编号与结构，只做纠错与补全。
4. 每篇至少 1 张图（ASCII 或 Mermaid）；文内互链必须是可解析的相对路径。
5. 只改你被分配的那一个文件。**最后要报告「纠正了原文哪些错误描述」**——这是本轮的质量抓手。

## 3. 已核实的仓库底账（不要重复侦查，直接用；仍建议按需 `ls` 复核）

### 3.1 Rust crate 总数

根 `Cargo.toml` 的 `[workspace] members` = **18**：`crates/*` **14 个** + `ee/*` **4 个**（`ee/tabby-ui` 是前端，不在 Rust workspace 内）。

`crates/`：`aim-downloader` `hash-ids` `http-api-bindings` `llama-cpp-server` `ollama-api-bindings` `sqlx-migrate-validate` `tabby` `tabby-common` `tabby-crawler` `tabby-download` `tabby-git` `tabby-index` `tabby-index-cli` `tabby-inference`

`ee/` 目录有 6 个子目录 + LICENSE，但**只有 4 个是 Rust crate**：`tabby-db` `tabby-db-macros` `tabby-schema` `tabby-webserver`。
另外两个不是 cargo crate，文档里若按「crate/模块」并列它们就是错的：

- `ee/tabby-ui` —— React + Vite 前端 SPA
- `ee/tabby-email` —— **没有 `Cargo.toml`**，只有 `README.md` `components/` `emails/` `package.json`，是邮件模板工程；Rust 侧的 SMTP 发送在 `ee/tabby-webserver/src/service/email/`

### 3.2 `crates/tabby/src/` 真实文件（11 个模块 + 3 子目录）

```
download.rs  main.rs  otel.rs  serve.rs
routes/{chat.rs, completions.rs, events.rs, health.rs, metrics.rs, mod.rs, models.rs, server_setting.rs}
services/{code.rs, completion.rs, embedding.rs, event.rs, health.rs, mod.rs, tantivy.rs}
services/completion/{completion_prompt.rs, next_edit_prompt.rs}
services/model/mod.rs
services/structured_doc/{mod.rs, serper.rs, tantivy.rs}
```

常见误写：`crates/tabby/services/tantivy.rs`（**漏 `src`**）、`crates/tabby/src/routes/completions.rs` 是真名（不是 `completion.rs`）。

### 3.3 `crates/http-api-bindings/src/`

```
lib.rs  rate_limit.rs
chat/mod.rs
completion/{mod.rs, llama.rs, mistral.rs, openai.rs}
embedding/{mod.rs, azure.rs, llama.rs, mistral.rs, openai.rs}
```

**没有** `completion/new_provider.rs`。讲「如何新增 provider」时那是示例文件，必须标「示例」。

### 3.4 `ee/tabby-db/src/` 真实 27 个文件（原文表格大量错名，按此重写）

```
access_policy.rs  attachment.rs  cache.rs  conversions.rs  email_setting.rs  ingestion.rs
integrations.rs   invitations.rs  job_runs.rs  ldap_credential.rs  lib.rs  migration_tests.rs
notifications.rs  oauth_credential.rs  pages.rs  password_reset.rs  provided_repositories.rs
refresh_tokens.rs  repositories.rs  server_setting.rs  threads.rs  user_chats.rs
user_completions.rs  user_events.rs  user_groups.rs  users.rs  web_documents.rs
```

**原文写错/写无的（必须逐条改）**：

| 原文 | 真实情况 |
|---|---|
| `settings.rs` | 不存在；实为 `server_setting.rs`（网络/品牌/安全设置）+ `email_setting.rs`（SMTP） |
| `auth_providers.rs` | 不存在；分属 `oauth_credential.rs` + `ldap_credential.rs` |
| `server_info.rs` | 不存在（tabby-db 里没有服务器元数据表） |
| `user_group.rs` | 复数 `user_groups.rs` |
| `integration.rs` | 复数 `integrations.rs` |
| `ldap.rs` | 不存在；实为 `ldap_credential.rs`（`ldap.rs` 在 `ee/tabby-webserver/src/`） |
| `analytic.rs` `license.rs` | **不在 tabby-db**，它们是 `ee/tabby-webserver/src/service/` 与 `ee/tabby-schema/src/schema/` 的同名文件 |
| `message.rs` | 不存在；会话消息在 `threads.rs` |

### 3.5 `ee/tabby-webserver/src/`

```
lib.rs  hub.rs  ldap.rs  path.rs  rate_limit.rs  webserver.rs  jwt.rs
axum/  oauth/{mod.rs, github.rs, gitlab.rs, google.rs, oidc.rs}
routes/{mod.rs, ui.rs, oauth.rs, hub.rs, ingestion.rs, repositories/}
service/{access_policy.rs, analytic.rs, answer.rs+answer/, auth.rs+auth/, background_job/,
         context.rs, email/, embedding.rs, event_logger.rs, ingestion.rs, integration.rs,
         job.rs, license.rs, mod.rs, notification.rs, page.rs+page/, preset_web_documents_data.rs,
         repository/, retrieval.rs+retrieval/, setting.rs, snapshots/, thread.rs,
         user_event.rs, user_group.rs, utils/, web_documents.rs}
```

常见误写：`ee/tabby-webserver/oauth/github.rs`、`ee/tabby-webserver/ldap.rs`、`service/auth.rs`（**全部漏 `src`**）。注意这里 `user_group.rs` / `integration.rs` 是**单数**，与 tabby-db 的复数不同——不要反过来套。

`ee/tabby-schema/src/`：`lib.rs  dao.rs  env.rs  policy.rs  juniper/  schema/`。**没有 `.graphql` 文件**（schema 全靠 juniper derive 生成，文档若写「.graphql 文件清单」要改）。

### 3.6 `ee/tabby-db/migrations/` — up 文件 **49** 个（up/down 成对）

### 3.7 `clients/tabby-agent/src/`（原文的 `git.ts`/`document.ts`/`defaults.ts`/`websocket.ts` 都不存在）

```
index.ts  server.ts  env.ts  feature.ts  protocol.ts  status.ts  command.ts
codeLens.ts  codeSearch.ts  certsLoader.ts  telemetry.ts
chat/        {index.ts, global.ts, inlineEdit.ts, smartApply.ts, smartRange.ts,
              generateBranchName.ts, generateCommitMessage.ts, utils.ts, prompts/}
chat/prompts/{edit-command-insert.md, edit-command-replace.md, fix-spelling-and-grammar.md,
              generate-branch-name.md, generate-commit-message.md, generate-docs.md,
              generate-smart-apply.md, include-file-context-item.md,
              include-file-context-list.md, provide-smart-apply-line-range.md, index.d.ts}
codeCompletion/  config/{index.ts, configFile.ts, default.ts, default.test.ts, type.d.ts}
contextProviders/{declarationSnippets.ts, documentContexts.ts, editorOptions.ts,
                  editorVisibleRanges.ts, recentlyChangedCodeSearch.ts, workspace.ts, git/{index.ts, gitCommand.ts}}
dataStore/  extensions/  http/{tabbyApiClient.ts, proxy.ts, stream.ts, cloudApi.d.ts}
logger/  syntax/  utils/
```

要点：**没有 `contextProviders/index.ts`**（注册不在那）；`config/defaults.ts` 实为 **`default.ts`**（单数）；`http/websocket.ts` 不存在，SSE/流在 `http/stream.ts`；git provider 是**目录** `contextProviders/git/`。
提示词模板对应关系（原文六个名字全错）：inline-edit→`edit-command-insert.md`/`edit-command-replace.md`；fix-spelling→`fix-spelling-and-grammar.md`；docs-generation→`generate-docs.md`；smart-apply→`generate-smart-apply.md`；branch-name→`generate-branch-name.md`；commit-msg→`generate-commit-message.md`。

`clients/vscode/src/`：`extension.ts`（不是 `vscode/extension.ts`）`Config.ts` `ContextVariables.ts` `CodeActions.ts` `StatusBarItem.ts` `chat/` `code-action/{InlineEdit.ts, QuickFix.ts}` `inline-edit/{index.ts, quickPick.ts, util.ts, util.test.ts}` `commands/` `git/` `lsp/` 等。

### 3.8 `python/`（原文 13 那节大量凭空内容）

```
python/tabby/trainer.py
python/tabby-loadtest/{server.py, loadtest.js, run.sh, README.md, record.csv}
python/tabby-eval/modal/predict.py          ← tabby-eval 只剩这一个目录一个文件
python/tabby-eval/modal/tabby_python_client/  ← 自动生成的 API client
```

**`python/tabby-eval/` 下没有 `eval.py`、没有 `tabby/model/__init__.py`**，且 `git log --diff-filter=D` 显示这些文件**从未存在过**（不是被删的）⇒ 描述它们的段落要**重写为「modal 上的 e2e 评估入口」这一真实形态**，而不是换个路径。
真实测试锚点：`crates/tabby/tests/{goldentests.rs, goldentests_chat.rs, snapshots/}`。

### 3.9 `.github/workflows/`（原文 11 的 CI 表整段错）

真实 15 个：`ast-grep-lint.yml` `autofix-pnpm.yml` `autofix-python.yml` `autofix-rust.yml` `bloat.yml` `docker.yml` `gh-pages.yml` `gpt-translate.yml` `nightly.yml` `release-intellij.yml` `release-vim.yml` `release-vscode.yml` `release.yml` `test-intellij.yml` `test-pnpm.yml` `test-rust.yml`

**不存在** `ci.yml` `e2e.yml` `docker-publish.yml` `codegen.yml`（`release.yml` 存在）。原表里的「执行内容」要按真实 workflow 文件读一遍再写，不能沿用。
`.github/ISSUE_TEMPLATE/`：只有 `bug_report.md`、`feature_request.md`（**没有 `config_bug_report.md`**）。
根 `ci/`：`package-from-upstream.sh`、`prepare_build_environment.{sh,ps1}`。**没有 `backup-tabby.sh`、`bento.yml`** ⇒ 那类内容要标成「自建脚本示例」。

### 3.10 其他已核实数量

- `#[cfg(test)]` 之外的生产 `.unwrap()`：全仓 `.unwrap()` 共 1014 处，其中 788 在测试区
- `unsafe impl` 仅一处：`crates/tabby-index/src/code/languages.rs`
- proc-macro crate 仅一个：`ee/tabby-db-macros`（`#[proc_macro] query_paged_as`）
- `crates/tabby-index/src/` 子目录：`code/`（含 `intelligence/`、`snapshots/`）、`structured_doc/types/`
- `crates/tabby-git/src/`：`lib.rs commit.rs file_search.rs serve_git.rs grep/{mod.rs,output.rs,query.rs,searcher.rs}`（`serve_git.rs`/`grep/mod.rs`/`file_search.rs`/`commit.rs` 四个都在，原文漏 `src` 前缀要补全）
- `crates/tabby-crawler/src/`：`lib.rs llms_txt_parser.rs types.rs`；正文里 "Portions.js in Rust" 是无意义表述，真实依赖是 `readable-readability = "0.4.0"`

## 4. 允许的「不算错误」形态（别去修，但可顺手加全称）

树状图里的裸文件名、`~/.tabby/` 与 `$TABBY_*` 这类运行期路径、`config.toml`/`tabby.json` 这类配置名、`migrations/YYYY-MM-DD_*.sql` 占位符、省略号路径、跨仓库引用（已注明 upstream repo 的）、库名（`discord.py` 同类）。
说明性句子（「该文件已不存在」）**必须把否定词和被引用的路径写在同一行**，否则工具的 negated 分档是按行判定的。
