# 第四章：企业版 (Enterprise Edition)

## 4.1 EE Overview

### 目录结构

```
ee/
├── tabby-webserver/     # Full web server (Axum + GraphQL + Admin UI)
├── tabby-schema/        # GraphQL schema definitions (Juniper)
├── tabby-db/            # SQLite database layer (SQLx)
├── tabby-db-macros/     # Proc macros for DB operations
├── tabby-ui/            # React + Next 13 (App Router) Admin SPA
└── tabby-email/         # react-email 邮件模板工程（无 Cargo.toml）
```

注意：`ee/` 下有 6 个子目录，但**只有 4 个是 Rust crate**（`tabby-db` `tabby-db-macros` `tabby-schema` `tabby-webserver`）。
`tabby-ui` 是前端工程、`tabby-email` 是邮件模板工程（只有 `README.md` `components/` `emails/` `package.json`），二者都不在 Rust workspace 内；SMTP 发送的实现位于 `ee/tabby-webserver/src/service/email/mod.rs`。

### 编译与运行

```bash
# 包含企业版构建（ee 是 crates/tabby 的编译期 feature：ee = ["dep:tabby-webserver"]）
cargo build --release --features ee

# 运行时：只有带 ee 编译出来的二进制才会挂上 webserver
./target/release/tabby serve              # 默认启用 EE webserver
./target/release/tabby serve --no-webserver   # 隐藏的 ee-only 开关，关闭 EE webserver
```

EE **不是运行时自动探测**的：`crates/tabby/src/serve.rs` 里用 `#[cfg(feature = "ee")]` 包住 `webserver = Some(!args.no_webserver)`，没开 feature 时这段代码根本不存在。

### License Tiers

档位取值来自 `ee/tabby-schema/src/schema/license.rs` · `enum LicenseType`（只有 `Community` / `Team` / `Enterprise` 三个变体，代码里没有 "Free" / "Pro"）：

| Tier | 席位上限 | 管理员席位 | 代码门控 |
|------|---------|-----------|---------|
| **Community** | `min(seats, 5)`（`seat_limits_for_community_license` +0） | 1 | 无 license 时的默认档，`license_check` 任务直接跳过检查 |
| **Team** | `min(seats, 50)`（`seat_limits_for_team_license` +0） | 3 | 需有效 license |
| **Enterprise** | 不封顶（`seats` 原值） | 不限 | `updateOauthCredential` / `testLdapConnection` / `updateLdapCredential` / `updateSecuritySetting` 四条 mutation 用 `check_license(ctx, &[LicenseType::Enterprise])` 硬门控 |

license 的合法性状态另有 `enum LicenseStatus`（`Ok` / `Expired` / `SeatsExceeded`，同文件 +20）；`ensure_valid_license` 对后两者返回 `CoreError::InvalidLicense`。

---

## 4.2 ee/tabby-db — Database Layer

**职责**: SQLite 持久化存储，包含用户、OAuth、权限、文档等全部企业数据的 CRUD。

### 架构

```rust
// ee/tabby-db/src/lib.rs · struct DbConn +0
pub struct DbConn {
    pool: Pool<Sqlite>,      // SQLx 连接池
    cache: Arc<DbCache>,     // 进程内查询缓存
}
```

- **连接池**: SQLx 自带的 `SqlitePool`（**没有 r2d2**），WAL 通过 `sqlx::sqlite::SqliteJournalMode::Wal` 在 `DbConn::new` 里指定
- **迁移管理**: 49 组 up/down 脚本（编号 `0001` → `0050`，缺 `0032`），`ee/tabby-db/migrations/` 下共 98 个 `.sql` 文件
- **Migration Validation**: 启动时用 `sqlx-migrate-validate` crate 的 `Validate` 校验待执行迁移（`ee/tabby-db/src/lib.rs` · `DbConn::new`）

### Entity Modules (27 个文件)

`ee/tabby-db/src/` 实测 27 个 `.rs` 文件（含 `lib.rs`、`migration_tests.rs` 等非实体模块），下表列出去掉入口/工具文件后的数据模块：

| 模块文件 | 职责 |
|----------|------|
| `users.rs` | User CRUD + password hash/verify；权限用 `is_admin` 布尔列（没有 `role` 列） |
| `invitations.rs` | Registration invitations, token management |
| `oauth_credential.rs` | OAuth provider configuration storage |
| `email_setting.rs` | SMTP settings persistence |
| `repositories.rs` | Git repository registration [EE] |
| `access_policy.rs` | 只读访问授权：`source_id_read_access_policies` 表按 `(source_id, user_group_id)` 授权 |
| `user_groups.rs` | User group management（`user_groups` + `user_group_memberships`） |
| `ingestion.rs` | Ingestion queue / status tracking（表名是 `ingested_documents`） |
| `pages.rs` | Web Pages (Answer Engine persistent content) |
| `threads.rs` | Chat thread storage，会话消息 (`thread_messages` / `ThreadMessageDAO`) 也在这里 |
| `user_chats.rs` | Per-user chat history |
| `web_documents.rs` | Custom web document management |
| `job_runs.rs` | Background job run records |
| `notifications.rs` | User notification delivery |
| `integrations.rs` | GitHub/GitLab integration config |
| `ldap_credential.rs` | LDAP connection settings |
| `server_setting.rs` | Network/branding/security settings |
| `refresh_tokens.rs` | JWT refresh token 签发与撤销 |
| `password_reset.rs` | 密码重置 code 的生命周期 |
| `user_completions.rs` | 补全 view/select 事件与按天聚合（`UserCompletionDailyStatsDAO`） |
| `user_events.rs` | IDE 原始事件日志 |
| `provided_repositories.rs` | 第三方集成同步下来的仓库列表 |
| ... (5 more) | 另有 `attachment.rs`（会话附件结构体，无独立表）、`cache.rs`（进程内 `Cache<T>`）、`conversions.rs`、`lib.rs`、`migration_tests.rs` |

两点容易踩空的对照：

- **tabby-db 里用复数**（`user_groups.rs`、`integrations.rs`），而 `ee/tabby-webserver/src/service/` 里用**单数**（`user_group.rs`、`integration.rs`）——引用时必须带 crate 前缀，别把两边互相"修正"。
- **tabby-db 里没有 `analytic.rs`、`license.rs`、`settings.rs`、`auth_providers.rs`、`server_info.rs`、`message.rs`**。分析聚合的服务实现在 `ee/tabby-webserver/src/service/analytic.rs`（schema 侧是 `ee/tabby-schema/src/schema/analytic.rs`）；license 没有独立表，只以 `server_setting.billing_enterprise_license` 一列存储，服务在 `ee/tabby-webserver/src/service/license.rs`；OAuth / LDAP 两套登录凭据分别落在 `oauth_credential.rs` 与 `ldap_credential.rs`，`authProviders` 只是 schema 层把两张凭据表拼出来的查询（`ee/tabby-schema/src/schema/mod.rs` · `auth_providers` +0）；`serverInfo` 同理，是在 schema 层现算的（同文件 · `server_info` +0），不是表；会话消息 (`thread_messages`) 与 `ThreadMessageDAO` 都在 `threads.rs` 里。

### Migration History (49 组 up/down，共 98 个文件)

Migration 按时间顺序演进，核心里程碑（编号取自 `ee/tabby-db/migrations/` 实际文件名，`0032` 缺号）：

| 迁移范围 | 核心变更 |
|----------|---------|
| 0001-0010 | Registration token、Users、Invitations、Refresh tokens、Job runs、GitHub/Google OAuth 凭据表、Email setting、Repositories、Server setting |
| 0011-0020 | Email settings 重构、SMTP port、Password reset、**Enterprise license 列 (0015)**、OAuth 三表合一 (0016)、User completions、密码可空、用户头像、Job run index |
| 0021-0030 | Repository name index、GitHub provider、User events (0025)、GitLab provider (0028)、Provider 表合并 (0029)、User name |
| 0031-0040 | Web crawler urls、Job runs 参数/started_at、**Thread + message 表 (0035)**、Web document (0036)、**Access control 三表 (0038)**、Notification inbox (0039)、Code source_id 列 (0040) |
| 0041-0050 | **LDAP credential (0041)**、Attachment file list、**Page section (0043)**、Page sources、Disable password login、**Ingestion (0047)**、Branding 列、OAuth 列、Repository refs |

---

## 4.3 ee/tabby-schema — GraphQL Schema

**职责**: Juniper GraphQL schema 定义，暴露所有企业级功能为统一 API。

### Schema Structure

```
ee/tabby-schema/src/
├── lib.rs              # Entry: exposes dao, env, policy, schema, juniper
├── dao.rs  env.rs  policy.rs
├── schema/
│   ├── mod.rs          # Core Query + Mutation + Subscription structs
│   ├── auth.rs         # JWT payload / OAuth & LDAP credential types (JWTPayload)
│   ├── thread.rs       # Thread service trait
│   ├── thread/         # types.rs + inputs.rs —— Message / CreateMessageInput 等消息类型都在这里
│   ├── page.rs  page/types.rs  repository/{mod,git,third_party,types}.rs
│   ├── user_group.rs  integration.rs   # 注意：schema 层用单数，与 tabby-db 的复数不同
│   └── access_policy.rs analytic.rs context.rs constants.rs email.rs ingestion.rs
│       interface.rs job.rs license.rs notification.rs retrieval.rs setting.rs
│       user_event.rs web_documents.rs worker.rs
└── juniper/relay/      # Relay-style pagination helpers
    ├── connection.rs   # Connection<Edge, PageInfo>
    ├── edge.rs  mod.rs  node_type.rs
    └── page_info.rs
```

`schema/` 与 `juniper/` 下**没有任何 `.graphql` 文件**：整份 GraphQL SDL 由 juniper 的 `#[graphql_object]` / `#[derive(GraphQLObject)]` 在编译期生成。

### ServiceLocator Trait — 核心服务目录

这是 EE 的**中枢接口**，管理所有业务服务的生命周期（`ee/tabby-schema/src/schema/mod.rs` · `pub trait ServiceLocator` +0）。注意它是**方法形式**（返回 `Arc<dyn …>`，可选服务返回 `Option<Arc<dyn …>>`），不是关联类型：

```rust
pub trait ServiceLocator: Send + Sync {
    fn auth(&self) -> Arc<dyn AuthenticationService>;
    fn worker(&self) -> Arc<dyn WorkerService>;
    fn code(&self) -> Option<Arc<dyn CodeSearch>>;
    fn chat(&self) -> Option<Arc<dyn ChatCompletionStream>>;
    fn completion(&self) -> Option<Arc<dyn CompletionStream>>;
    fn embedding(&self) -> Option<Arc<dyn EmbeddingService>>;
    fn logger(&self) -> Arc<dyn EventLogger>;
    fn ingestion(&self) -> Arc<dyn IngestionService>;
    fn job(&self) -> Arc<dyn JobService>;
    fn repository(&self) -> Arc<dyn RepositoryService>;
    fn integration(&self) -> Arc<dyn IntegrationService>;
    fn email(&self) -> Arc<dyn EmailService>;
    fn setting(&self) -> Arc<dyn SettingService>;
    fn license(&self) -> Arc<dyn LicenseService>;
    fn analytic(&self) -> Arc<dyn AnalyticService>;
    fn user_event(&self) -> Arc<dyn UserEventService>;
    fn web_documents(&self) -> Arc<dyn WebDocumentService>;
    fn thread(&self) -> Arc<dyn ThreadService>;
    fn page(&self) -> Option<Arc<dyn PageService>>;
    fn context(&self) -> Arc<dyn ContextService>;
    fn user_group(&self) -> Arc<dyn UserGroupService>;
    fn access_policy(&self) -> Arc<dyn AccessPolicyService>;
    fn notification(&self) -> Arc<dyn NotificationService>;
}
```

共 23 个方法；实现方在 `ee/tabby-webserver/src/service/mod.rs` · `ServerContext`。`code` / `chat` / `completion` / `embedding` / `page` 五个是 `Option`，取决于启动时是否配置了对应能力。

### GraphQL Queries (43 个)

定义位置：`ee/tabby-schema/src/schema/mod.rs` · `struct Query` 的 `#[graphql_object] impl`（+0 起，字段名按 juniper 规则从 snake_case 转 camelCase）。

| Query | 返回 |
|-------|------|
| `registrationToken` | Admin registration token |
| `me` | Current authenticated user |
| `users(...)` | User list (paginated) |
| `invitations(...)` | Invitation list |
| `gitRepositories(...)` | Registered Git repos |
| `repositoryGrep(kind:, id:, rev:, query:)` | Code grep in one repository |
| `emailSetting` | SMTP configuration |
| `networkSetting` / `securitySetting` / `brandingSetting` | Server settings |
| `authProviders` | OAuth provider list |
| `oauthCredential(provider:)` | OAuth client config |
| `serverInfo` | Version + feature flags |
| `license` | Current license info |
| `dailyStats(start:, end:, users:, languages:)` | `Vec<CompletionStats>`（按语言分天的 completions/views/selects） |
| `userEvents(...)` | Event log (impression/click) |
| `notifications` | User notifications（无参数，返回 `Vec`） |
| `diskUsageStats()` | Database/storage usage |
| `integrations(...)` | GitHub/GitLab integrations |
| `ingestionStatus(sources:)` | Document ingestion status（入参是 source id 列表） |
| `threads(...)` | Chat thread list |
| `pages(...)` | Web page list |
| `customWebDocuments(...)` / `presetWebDocuments` | Custom / preset document list |
| `userGroups` | User group list（无参数） |
| `sourceIdAccessPolicies(sourceId:)` | Access control rules for one source id |
| ... (18 more) | `chatDailyStats` `chatDailyStatsInPastYear` `dailyStatsInPastYear` `integratedRepositories` `jobRuns` `jobRunStats` `jobs` `ldapCredential` `myThreads` `oauthCallbackUrl` `pageSections` `readRepositoryRelatedQuestions` `repositoryList` `repositorySearch` `testModelConnection` `threadMessages` `contextInfo` |

### GraphQL Mutations (62 个)

| Mutation | 功能 |
|----------|------|
| `register(email:, password1:, password2:, name:, invitationCode:)` | Admin registration |
| `tokenAuth(email:, password:)` | Login → JWT token（LDAP 走 `tokenAuthLdap`） |
| `passwordReset(input: PasswordResetInput)` | Password reset flow（`code` + `password1`/`password2`） |
| `createGitRepository(name:, gitUrl:, refs:)` | Register new Git repo |
| `updateOauthCredential(input:)` | Update OAuth config（**没有** `createOauthCredential`） |
| `triggerJobRun(command:)` | Trigger background job（入参是命令字符串，不是枚举） |
| `deletePage(id:)` | Delete web page |
| `createCustomDocument(input:)` / `deleteCustomDocument(id:)` | Add / remove custom doc |
| `createUserGroup(input:)` / `deleteUserGroup(id:)` | Create / delete user group |
| `upsertUserGroupMembership(input:)` / `deleteUserGroupMembership(userGroupId:, userId:)` | 组成员管理 |
| `grantSourceIdReadAccess(sourceId:, userGroupId:)` / `revokeSourceIdReadAccess(...)` | RBAC control（**没有** `grantAccessPolicy` / `revokeAccessPolicy`） |
| ... (50 more) | 例如 `uploadLicense` `resetLicense` `sendTestEmail` `updateUserActive` `updateUserRole` `deleteInvitation` `resetRegistrationToken` `updateNetworkSetting` `updateSecuritySetting` `updateBrandingSetting` `updateLdapCredential` `testLdapConnection` `createIntegration` `updateIntegration` `deleteIntegration` `markNotificationsRead` `createThreadRun` 系列的写侧配套等 |

### GraphQL Subscriptions (5 个)

定义位置：`ee/tabby-schema/src/schema/mod.rs` · `struct Subscription`。五个订阅的入参统一是 `input: Create*Input`，返回的都是 `BoxStream<Result<…Item>>`。

```graphql
# Real-time thread interaction
subscription {
  createThreadAndRun(input: CreateThreadAndRunInput!) {
    ... on ThreadUserMessageCreated { id }
  }
}

subscription {
  createThreadRun(input: CreateThreadRunInput!) {
    # ThreadRunItem 是一个 GraphQL union，共 11 个成员，必须用 inline fragment 选字段
    ... on ThreadAssistantMessageContentDelta { delta }
    ... on ThreadAssistantMessageCompleted { debugData }
  }
}

# Similar for Pages
subscription {
  createPageRun(input: CreatePageRunInput!) { ... }
  createPageSectionRun(input: CreatePageSectionRunInput!) { ... }
}

# 把一次会话固化成 Page
subscription {
  createThreadToPageRun(input: CreateThreadToPageRunInput!) { ... }
}
```

union 的 11 个成员定义在 `ee/tabby-schema/src/schema/thread/types.rs` · `enum ThreadRunItem` +0；流里除 `threadAssistantMessageContentDelta` 会出现多次，其余成员各只出现一次。

---

## 4.4 ee/tabby-webserver — Web Server

**职责**: 将 GraphQL Schema + DB Service + UI SPA 组合为完整可用的企业管理后端。

### Architecture

```
ee/tabby-webserver/src/
├── lib.rs              # Module declarations: axum, hub, jwt, ldap, oauth, path, rate_limit, routes, service, webserver
├── webserver.rs        # Webserver struct: orchestrates all services via attach()
├── service/
│   ├── mod.rs          # ServiceLocator impl + ServerContext + authorization middleware
│   ├── auth.rs  auth/  # JWT authentication (password + OAuth flows)
│   ├── email/          # SMTP service
│   │   ├── mod.rs      # Email sender (lettre AsyncSmtpTransport)
│   │   └── templates.rs  # include_str! 载入 ../../email_templates/*.html
│   ├── license.rs      # License validation & caching
│   ├── setting.rs      # Network/branding/security settings CRUD
│   ├── repository/
│   │   ├── git.rs      # Git repo management (create/update/delete/list)
│   │   ├── third_party.rs  # Third-party repo sync
│   │   ├── third_party/  prompt_tools.rs  mod.rs
│   ├── integration.rs  # GitHub/GitLab integration lifecycle
│   ├── ingestion.rs    # Custom doc ingestion API
│   ├── web_documents.rs # Web document CRUD + preset_web_documents_data.rs
│   ├── retrieval.rs  retrieval/  # Code+doc unified retrieval service
│   ├── answer.rs  answer/  # Answer Engine service + prompt tools
│   ├── page.rs  page/  # Page management (persistent Answer Engine content)
│   ├── thread.rs       # Chat thread management
│   ├── user_group.rs   # User group CRUD
│   ├── access_policy.rs # Access control rules
│   ├── analytic.rs     # Analytics data aggregation
│   ├── event_logger.rs # User event logging
│   ├── job.rs  background_job/  context.rs  embedding.rs  notification.rs
│   ├── user_event.rs  snapshots/  utils/  mod.rs
│   └── (service/ 下实测 56 个 .rs 文件)
│   ├── background_job/         # 后台任务实现（在 service/ 之下，不是 src/job/）
│   │   ├── mod.rs              # start()：hourly / daily / 每 10 秒 三条 CronStream
│   │   ├── daily.rs  hourly.rs # DailyJob 负责调度 LicenseCheckJob
│   │   ├── index_commits.rs  index_pages.rs  index_ingestion.rs
│   │   ├── index_garbage_collection.rs  web_crawler.rs
│   │   ├── license_check.rs    # 到期前 7 天给管理员发通知
│   │   ├── git.rs  db.rs  helper/
│   │   ├── third_party.rs
│   │   └── third_party_integration/  # 只有 error.rs / issues.rs / pulls.rs
│   │                                 # 没有 github.rs / gitlab.rs，provider 差异按 kind 分派
├── routes/             # Axum 路由: mod.rs, ui.rs, oauth.rs, hub.rs, ingestion.rs, repositories/
├── axum/               # Axum 公共helpers
├── path.rs             # EE 数据目录 (~/.tabby/ee)
├── rate_limit.rs       # UserRateLimiter
├── hub.rs              # WebSocket hub (real-time features)
├── jwt.rs              # JWT token generation/validation
├── ldap.rs             # LDAP authentication module
└── oauth/              # OAuth provider implementations
    ├── mod.rs
    ├── github.rs       # GitHub OAuth2
    ├── gitlab.rs       # GitLab OAuth2
    ├── google.rs       # Google OAuth2
    └── oidc.rs         # OpenID Connect (generic)
```

（`ee/tabby-webserver/src` 下含子目录共 78 个 `.rs`；顶层另有 `development/` `docs/` `keys/` `email_templates/` `ui/` 等非源码目录。）

### Authorization Middleware

`/v1/` 与 `/v1beta/` 前缀下的 REST 请求由 `ServerContext::authorize_request`（`ee/tabby-webserver/src/service/mod.rs` · `authorize_request` +0）统一鉴权，它由 axum 中间件入口 `WorkerService::dispatch_request`（同文件 · `dispatch_request` +0）调用：

```rust
// ee/tabby-webserver/src/service/mod.rs · authorize_request +0
async fn authorize_request(&self, uri: &Uri, headers: &HeaderMap) -> (bool, Option<ID>) {
    // +2   非 /v1/ 与 /v1beta/ 路径直接放行，返回 (true, None)
    // +7   从 Authorization 头切出 bearer token
    // +15  没有 token 则拒绝
    // +21  先试 JWT（verify_access_token）—— 有 license 与否都放行
    // +27  否则用 db 里的 auth token：license 失效或 demo 模式时只允许 owner token
}
```

注意它的返回是 `(是否放行, 可选用户 ID)`，**不做 RBAC 判定**——资源级读写权限在 GraphQL 层由各 resolver 里的 `check_admin` / `check_user` + `user.policy.check_*` 完成。

### Rate Limiting

- 实现：`ee/tabby-webserver/src/rate_limit.rs` · `struct UserRateLimiter` +0，用 `ratelimit` crate + `cached::TimedCache` 按 **user id** 维护令牌桶
- 默认值：`Module:ee/tabby-webserver/src/rate_limit.rs` · `USER_REQUEST_LIMIT_PER_MINUTE = 120`，**硬编码、不可按模型配置**；桶容量 `120 * 2`，缓存条目寿命 60 秒
- 例外：`/v1/health` 与 `/v1beta/health` 直接放行（同文件 · `UserRateLimiter::is_allowed` +2）
- 挂载点：不是 tower layer，而是在 `dispatch_request` 内部对已鉴权用户逐请求调用

---

## 4.5 ee/tabby-ui — Frontend SPA

**技术栈**: React 18 + **Next 13（App Router）** + TypeScript + Tailwind CSS（依赖见 `ee/tabby-ui/package.json`，`next ^13.4.7`；`vitest` 只是测试运行器，构建不是 Vite）

### Admin Panel 页面结构

路由取自 `ee/tabby-ui/app/` 下的 32 个 `page.tsx`；`(dashboard)`、`(home)`、`(logs)`、`(integrations)` 是 Next 的 route group，**不出现在 URL 里**，所以没有 `/admin/...` 前缀。

| 页面路由 | 源目录 | 功能 |
|---------|--------|------|
| `/` | `app/(home)` | 会话首页（chat feeds） |
| `/auth/signin` | `app/auth/signin` | Login (tokenAuth / OAuth redirect) |
| `/auth/signup` | `app/auth/signup` | Initial admin / invited registration |
| `/auth/reset-password` | `app/auth/reset-password` | 密码重置落地页 |
| `/chat` `/search` `/files` `/pages` | `app/chat` 等 | 使用侧界面：问答 / 代码搜索 / 仓库浏览 / Pages |
| `/reports` | `app/(dashboard)/reports` | Usage analytics dashboard |
| `/activities` | `app/(dashboard)/activities` | User events 列表 |
| `/jobs` `/jobs/detail` | `app/(dashboard)/(logs)/jobs` | 后台任务运行记录 |
| `/system` | `app/(dashboard)/system` | 系统 / 磁盘占用信息 |
| `/experiments` | `app/(dashboard)/experiments` | Experiment flags |
| `/profile` | `app/(dashboard)/profile` | 当前用户资料与 token |
| `/settings/general` | `app/(dashboard)/settings/general` | Network/branding/security settings |
| `/settings/subscription` | `app/(dashboard)/settings/subscription` | License upload + validation |
| `/settings/team` `/settings/team/groups` | `app/(dashboard)/settings/team` | User management + invites / user groups |
| `/settings/providers/git` `/settings/providers/doc` | `.../(integrations)/providers` | Git repo / 文档源注册（含 `[kind]`、`new`、`detail` 子页） |
| `/settings/sso` `/settings/sso/new` `/settings/sso/detail/ldap` | `.../(integrations)/sso` | SSO/OAuth 与 LDAP configuration |
| `/settings/mail` | `.../(integrations)/mail` | SMTP settings |
| （通知中心） | `ee/tabby-ui/components/notification-box.tsx` | 铃铛组件，**没有独立路由页** |

原表里的 `/login` `/register` `/dashboard` `/documents` `/threads`，以及全部 `/admin/...` 前缀路由（`/admin/users` `/admin/invitations` `/admin/models` `/admin/repository` `/admin/integration` `/admin/sso` `/admin/email` `/admin/license` `/admin/access-control` `/admin/analytics` `/admin/settings` `/admin/notifications`）都不存在；只有 `/pages` 是真的。模型配置也不在 tabby-ui 里（无 `/settings/models` 页，模型由 `config.toml` / 启动参数决定）。

---

## 4.6 ee/tabby-email — Email Templates

**职责**: 用 react-email 编写邮件模板并预览；**它不是 Rust crate**（目录下只有 `README.md` `components/` `emails/` `package.json`，没有 `Cargo.toml`），SMTP 发送与 HTML 渲染都在 `ee/tabby-webserver/src/service/email/`。

### 模板流水线（照 `ee/tabby-email/README.md`）

```
ee/tabby-email/emails/*.tsx      —— 用 react-email 写模板，yarn dev 可预览
        │  make update-email-templates
        ▼
ee/tabby-webserver/email_templates/*.html   —— 生成出来的静态 HTML
        │  include_str!
        ▼
ee/tabby-webserver/src/service/email/templates.rs  —— 占位符替换（{{NAME}} → 实参）
        ▼
ee/tabby-webserver/src/service/email/mod.rs —— EmailService::send_invitation / send_signup /
                                              send_password_reset / send_test
```

### Template Types

`ee/tabby-email/emails/` 与 `ee/tabby-webserver/email_templates/` 实测各 4 个模板（外加 `ee/tabby-email/components/root-layout.tsx`）：

| 模板 | 触发场景 |
|------|---------|
| `invitation.tsx` → `invitation.html` | Admin 邀请新用户注册 |
| `password_reset.tsx` → `password_reset.html` | 用户密码重置链接 |
| `signup_success.tsx` → `signup_success.html` | 注册成功通知（**没有** `welcome.html`） |
| `test.tsx` → `test.html` | `sendTestEmail` mutation 的测试邮件 |

---

## 4.7 Enterprise Features Summary Table

### 功能矩阵

| 功能 | EE Component | API Surface | 配置位置 |
|------|-------------|-------------|---------|
| **用户管理** | `ee/tabby-db/src/users.rs` | GraphQL `users`, `register` | `/settings/team` |
| **OAuth SSO** | `ee/tabby-webserver/src/oauth/`（凭据表 `ee/tabby-db/src/oauth_credential.rs`） | GraphQL `authProviders`, `updateOauthCredential` | `/settings/sso` |
| **LDAP** | `ee/tabby-webserver/src/ldap.rs`（凭据表 `ee/tabby-db/src/ldap_credential.rs`） | GraphQL `ldapCredential`, `updateLdapCredential`, `testLdapConnection` | `/settings/sso/detail/ldap` |
| **RBAC** | `ee/tabby-webserver/src/service/access_policy.rs` + `user_group.rs`（表侧是复数 `ee/tabby-db/src/user_groups.rs`） | GraphQL `userGroups`, `sourceIdAccessPolicies`, `grantSourceIdReadAccess` | `/settings/team/groups` |
| **License 管理** | `ee/tabby-webserver/src/service/license.rs`（无独立表，落 `server_setting.billing_enterprise_license`） | GraphQL `license`, `uploadLicense` | `/settings/subscription` |
| **Email SMTP** | `ee/tabby-webserver/src/service/email/`（模板 `ee/tabby-email/emails/`） | GraphQL `emailSetting`, `updateEmailSetting`, `sendTestEmail` | `/settings/mail` |
| **Usage Analytics** | `ee/tabby-webserver/src/service/analytic.rs` + `ee/tabby-db/src/user_completions.rs` | GraphQL `dailyStats`, `chatDailyStats`, `userEvents` | `/reports` `/activities` |
| **Git/GitLab 集成** | `ee/tabby-webserver/src/service/integration.rs` + `ee/tabby-webserver/src/service/repository/third_party.rs`（表侧 `ee/tabby-db/src/integrations.rs`） | GraphQL `integrations`, `createIntegration` | `/settings/providers/git` |
| **自定义文档摄取** | `ee/tabby-webserver/src/service/ingestion.rs` + `crates/tabby-crawler/` | GraphQL `ingestionStatus`, `customWebDocuments` | `/settings/providers/doc` |
| **Web Pages** | `ee/tabby-webserver/src/service/page.rs` | GraphQL `pages`, Subscriptions | `/pages` |
| **Chat Threads** | `ee/tabby-webserver/src/service/thread.rs` + `ee/tabby-db/src/user_chats.rs` `threads.rs` | GraphQL `threads`, Subscriptions | `/chat` |
| **通知系统** | `ee/tabby-webserver/src/service/notification.rs` + `ee/tabby-db/src/notifications.rs` | GraphQL `notifications`, `markNotificationsRead` | 顶栏铃铛（`ee/tabby-ui/components/notification-box.tsx`） |
| **Usage Reporting** | `ee/tabby-webserver/src/service/background_job/hourly.rs` `daily.rs` + `ee/tabby-webserver/src/service/analytic.rs` | Reports page | `/reports` |

---

## 4.8 EE Database Schema Overview

### Key Tables

`ee/tabby-db/migrations/` 的 `CREATE TABLE` 共出现 **33** 个表名；其中 7 个（`github_oauth_credential` `google_oauth_credential` `github_repository_provider` `gitlab_repository_provider` `github_provided_repositories` `gitlab_provided_repositories` `web_crawler_urls`）已被 0016 / 0029 / 0037 等迁移合并或删除，**当前存活 26 张**。表名**没有下划线前缀**（列名才常带 `smtp_` / `network_` 之类前缀）。

```sql
-- Users & Auth
users                  -- (id, email, name, password_encrypted, is_admin, auth_token, active, created_at, updated_at)
                       --   注意：库里是 is_admin 布尔列，GraphQL 的 role("admin"/"user") 是在 DAO 之上派生的
registration_token     -- 初始注册 token
invitations            -- (id, email, code, created_at)
refresh_tokens         -- (id, user_id, token, expires_at, created_at)
password_reset         -- (user_id, code, created_at)
oauth_credential       -- (provider, config_url, config_scopes, client_id, client_secret)  三张分表在 0016 合并成这一张
ldap_credential        -- (host, port, bind_dn, bind_password, base_dn, user_filter, encryption,
                       --  skip_tls_verify, email_attribute, name_attribute)   单行，id 恒为 1

-- Repository & Integration
repositories           -- (id, name, git_url, refs)          —— GraphQL 里叫 gitRepositories
integrations           -- (id, kind, display_name, access_token, api_base, error, synced)
provided_repositories  -- (id, vendor_id, integration_id, name, git_url, refs, active)

-- Access Control
user_groups            -- (id, name, created_at, updated_at)          —— 没有 description 列
user_group_memberships -- (id, user_id, user_group_id, is_group_admin)
source_id_read_access_policies -- (id, source_id, user_group_id)
                       --   只有「某个 source_id 对某个组可读」这一种授权，
                       --   没有 resource_type / resource_id / allowed 这些列

-- Documents & Ingestion
pages                  -- (id, author_id, title, code_source_id, content)
page_sections          -- (id, page_id, title, content)
web_documents          -- (id, name, url, is_preset)
ingested_documents     -- (id, source, doc_id, title, body, link, expired_at)   摄取队列/状态

-- Analytics & Events
user_completions       -- (user_id, completion_id, language, views, selects, dismisses, created_at)
user_events            -- (id, user_id, kind, payload, created_at)
                       --   没有 daily_stats / analytics_snapshot 表：日聚合是查询时现算的
                       --   （ee/tabby-db/src/user_completions.rs 的 UserCompletionDailyStatsDAO 等）

-- Email & Settings
email_setting          -- (id, smtp_username, smtp_password, smtp_server, smtp_port,
                       --  from_address, encryption, auth_method)
server_setting         -- 单行：network_external_url, branding_logo, branding_icon,
                       --   security_allowed_register_domain_list, security_disable_client_side_telemetry,
                       --   security_disable_password_login, billing_enterprise_license
                       --   ↑ license 只有这一列，没有 _license 表；设置也不是 JSON blob，是一列一个字段

-- Notifications
notifications          -- (id, recipient, content, read, created_at, updated_at)
read_notifications     -- 已读关系表

-- Chat
threads                -- (id, user_id, is_ephemeral, relevant_questions, created_at, updated_at)
thread_messages        -- (id, thread_id, role, content, ...)
                       --   没有 user_chats 表：ee/tabby-db/src/user_chats.rs 是从 user_events 聚合聊天统计

-- Background jobs
job_runs               -- (id, name, command, exit_code, stdout, started_at, finished_at)
```

**不存在**的表（原文出现过）：`_users` `_invitations` `_refresh_tokens` `_oauth_credentials` `_ldap_setting` `_git_repositories` `_third_party_sync_status` `_access_policies` `_user_group_members` `_commit_indexing_status` `_daily_stats` `_analytics_snapshot` `_server_settings` `_license` `_user_chats`。
