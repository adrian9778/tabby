# 第四章：企业版 (Enterprise Edition)

## 4.1 EE Overview

### 目录结构

```
ee/
├── tabby-webserver/     # Full web server (Axum + GraphQL + Admin UI)
├── tabby-schema/        # GraphQL schema definitions (Juniper)
├── tabby-db/            # SQLite database layer (SQLx)
├── tabby-db-macros/     # Proc macros for DB operations
├── tabby-ui/            # React + Vite Admin SPA
└── tabby-email/         # SMTP rendering + email templates
```

### 编译与运行

```bash
# 包含企业版构建
cargo build --release --features ee

# 运行时自动启用 EE features
./target/release/tabby serve     # Auto-detects ee feature
```

### License Tiers

| Tier | 功能范围 | 适用场景 |
|------|---------|---------|
| **Free** | Core REST API + Open Source crates | 个人开发者、小型团队 |
| **Pro** | EE GraphQL API + Auth + RBAC + Analytics | 中小型企业 |
| **Enterprise** | All EE features + LDAP + Custom License | 大型企业、合规要求 |

---

## 4.2 ee/tabby-db — Database Layer

**职责**: SQLite 持久化存储，包含用户、OAuth、权限、文档等全部企业数据的 CRUD。

### 架构

```rust
pub struct DbConn {
    pool: r2d2::Pool<SqliteConnectionManager>,  // Connection pool (WAL mode)
}
```

- **连接池**: SQLx built-in with r2d2, WAL mode for concurrent reads
- **迁移管理**: 50 SQL migration files (0001 → 0050)
- **Migration Validation**: `sqlx-migrate-validate` crate in compile-time

### Entity Modules (27 个)

| 模块文件 | 职责 |
|----------|------|
| `users.rs` | User CRUD, password hash/verify, role (admin/user) |
| `invitations.rs` | Registration invitations, token management |
| `oauth_credential.rs` | OAuth provider configuration storage |
| `email_setting.rs` | SMTP settings persistence |
| `repositories.rs` | Git repository registration [EE] |
| `access_policy.rs` | RBAC access control rules |
| `user_group.rs` | User group management |
| `ingestion.rs` | Ingestion queue / status tracking |
| `pages.rs` | Web Pages (Answer Engine persistent content) |
| `threads.rs` | Chat thread storage |
| `user_chats.rs` | Per-user chat history |
| `web_documents.rs` | Custom web document management |
| `job_runs.rs` | Background job run records |
| `notifications.rs` | User notification delivery |
| `analytic.rs` | Usage statistics aggregation |
| `license.rs` | License key + validation cache |
| `settings.rs` | Network/branding/security settings |
| `integration.rs` | GitHub/GitLab integration config |
| `ldap.rs` | LDAP connection settings |
| `auth_providers.rs` | Auth provider registration |
| `server_info.rs` | Server metadata (version, features) |
| ... (10+ more small modules) | |

### Migration History (50 files)

Migration 按时间顺序演进，核心里程碑：

| 迁移范围 | 核心变更 |
|----------|---------|
| 0001-0010 | Registration tokens, Users, Invitations, Refresh Tokens |
| 0011-0020 | OAuth Providers, Email Settings, Repositories, Job Runs |
| 0021-0030 | LDAP Support, Web Documents, Pages |
| 0031-0040 | Access Control (RBAC), Notifications, User Groups |
| 0041-0050 | Threads, Ingestion, Final entity additions |

---

## 4.3 ee/tabby-schema — GraphQL Schema

**职责**: Juniper GraphQL schema 定义，暴露所有企业级功能为统一 API。

### Schema Structure

```
ee/tabby-schema/src/
├── lib.rs              # Entry: exposes dao, schema, juniper
├── schema/
│   ├── mod.rs          # Core Query + Mutation + Subscription structs
│   ├── jwt.rs          # JWT token operations
│   ├── message.rs      # Message entity types
│   └── ...
└── juniper/relay/      # Relay-style pagination helpers
    ├── connection.rs   # Connection<Edge, PageInfo>
    ├── edge.rs
    └── page_info.rs
```

### ServiceLocator Trait — 核心服务目录

这是 EE 的**中枢接口**，管理所有业务服务的生命周期：

```rust
pub trait ServiceLocator: Send + Sync + 'static {
    type Auth: AuthService;           // Authentication service
    type Worker: WorkerService;        // Core worker (completion, embedding, etc.)
    type Code: CodeService;            // Code search service
    type Chat: ChatService;            // Chat/Answer service
    type Completion: CompletionService;
    type Embedding: EmbeddingService;
    type Logger: LoggerService;
    type Ingestion: IngestionService;
    type Job: JobService;              // Background job management
    type Repository: RepositoryService;
    type Integration: IntegrationService;  // GitHub/GitLab integration
    type Email: EmailService;          // SMTP email service
    type Setting: SettingService;      // Server settings
    type License: LicenseService;      // License key management
    type Analytic: AnalyticService;    // Usage analytics
    type UserEvent: UserEventService;  // IDE usage events
    type WebDocuments: WebDocumentService;
    type Thread: ThreadService;
    type Page: PageService;
    type Context: ContextService;
    type UserGroup: UserGroupService;
    type AccessPolicy: AccessPolicyService;
    type Notification: NotificationService;
}
```

### GraphQL Queries (~60 个)

| Query | 返回 |
|-------|------|
| `registrationToken` | Admin registration token |
| `me` | Current authenticated user |
| `users(...)` | User list (paginated) |
| `invitations(...)` | Invitation list |
| `gitRepositories(...)` | Registered Git repos |
| `repositoryGrep(query:, repoId:)` | Code grep across repos |
| `emailSetting` | SMTP configuration |
| `networkSetting` / `securitySetting` / `brandingSetting` | Server settings |
| `authProviders` | OAuth provider list |
| `oauthCredential(provider:)` | OAuth client config |
| `serverInfo` | Version + feature flags |
| `license` | Current license info |
| `dailyStats(user:, since:, until:)` | Usage analytics daily stats |
| `userEvents(...)` | Event log (impression/click) |
| `notifications(...)` | User notifications |
| `diskUsageStats()` | Database/storage usage |
| `integrations(...)` | GitHub/GitLab integrations |
| `ingestionStatus(url:)` | Document ingestion status |
| `threads(...)` | Chat thread list |
| `pages(...)` | Web page list |
| `webDocuments(...)` | Custom document list |
| `userGroups(...)` | User group list |
| `accessPolicies(...)` | Access control rules |
| ... (35+ more) | |

### GraphQL Mutations (~45 个)

| Mutation | 功能 |
|----------|------|
| `register(name:, email:, password:)` | Admin registration |
| `tokenAuth(email:, password:)` | Login → JWT token |
| `passwordReset(token:, password:)` | Password reset flow |
| `createGitRepository(name:, gitUrl:)` | Register new Git repo |
| `updateOauthCredential(...)` | Update OAuth config |
| `triggerJobRun(jobType:)` | Trigger background job |
| `deletePage(id:)` | Delete web page |
| `createCustomDocument(url:)` | Add custom doc |
| `createUserGroup(name:)` | Create user group |
| `grantAccessPolicy(...)` / `revokeAccessPolicy(...)` | RBAC control |
| ... (30+ more) | |

### GraphQL Subscriptions (~4 个)

```graphql
# Real-time thread interaction
subscription {
  createThreadAndRun(input: ThreadInput) {
    id
    messages { content, role }
  }
}

subscription {
  createThreadRun(threadId: "xxx", message: "Hello") {
    delta       # Streaming text chunk
    done        # Boolean completion marker
  }
}

# Similar for Pages
subscription {
  createPageRun(...) { ... }
  createPageSectionRun(...) { ... }
}
```

---

## 4.4 ee/tabby-webserver — Web Server

**职责**: 将 GraphQL Schema + DB Service + UI SPA 组合为完整可用的企业管理后端。

### Architecture

```
ee/tabby-webserver/src/
├── lib.rs              # Module declarations (axum, hub, jwt, ldap, oauth, service, webserver)
├── webserver.rs        # Webserver struct: orchestrates all services via attach()
├── service/
│   ├── mod.rs          # ServiceLocator impl + ServerContext + authorization middleware
│   ├── auth.rs         # JWT authentication (password + OAuth flows)
│   ├── email/          # SMTP service
│   │   ├── mod.rs      # Email sender
│   │   └── templates/  # Invitation/Reset/Welcome HTML templates
│   ├── license.rs      # License validation & caching
│   ├── setting.rs      # Network/branding/security settings CRUD
│   ├── repository/
│   │   ├── git.rs      # Git repo management (create/update/delete/list)
│   │   └── third_party.rs  # Third-party repo sync
│   ├── integration.rs  # GitHub/GitLab integration lifecycle
│   ├── ingestion.rs    # Custom doc ingestion API
│   ├── web_documents.rs # Web document CRUD
│   ├── retrieval.rs    # Code+doc unified retrieval service
│   ├── answer.rs       # Answer Engine service + prompt tools
│   ├── page.rs         # Page management (persistent Answer Engine content)
│   ├── thread.rs       # Chat thread management
│   ├── user_group.rs   # User group CRUD
│   ├── access_policy.rs # Access control rules
│   ├── analytic.rs     # Analytics data aggregation
│   ├── notification.rs # Notification delivery
│   └── ... (49+ service files total)
├── job/                # Background job implementation
│   ├── daily.rs        # Daily stats generation
│   ├── hourly.rs       # Hourly metrics collection
│   ├── index_commits.rs     # Commit indexing trigger
│   ├── index_pages.rs     # Page indexing trigger
│   ├── index_ingestion.rs # Ingestion queue processor
│   ├── index_garbage_collection.rs # Index cleanup
│   ├── web_crawler.rs          # Web crawler scheduler
│   ├── third_party_integration/  # GitHub/GitLab sync jobs
│   │   ├── github.rs         # PR/Issue sync
│   │   └── gitlab.rs         # MR sync
│   ├── license_check.rs       # License validity check
│   ├── git.rs               # Git maintenance
│   └── db.rs                # SQLite vacuum/maintenance
├── hub.rs              # WebSocket hub (real-time features)
├── jwt.rs              # JWT token generation/validation
├── ldap.rs             # LDAP authentication module
├── oauth/              # OAuth provider implementations
│   ├── github.rs       # GitHub OAuth2
│   ├── gitlab.rs       # GitLab OAuth2
│   ├── google.rs       # Google OAuth2
│   └── oidc.rs         # OpenID Connect (generic)
└── ...
```

### Authorization Middleware

所有 EE API 请求通过统一的 `authorize_request` 中间件：

```rust
// service/mod.rs — authorize_request
pub async fn authorize_request(token: Option<Bearer>) -> Result<User, AuthError> {
    // 1. Extract JWT from Authorization header
    // 2. Validate signature + expiry
    // 3. Return authenticated user context
    // 4. RBAC check via AccessPolicyService
}
```

### Rate Limiting

- Per-endpoint rate limiting via `ratelimit` crate
- Default: 1200 req/min (configurable per model)
- Implemented as tower layer or axum middleware

---

## 4.5 ee/tabby-ui — Frontend SPA

**技术栈**: React + Vite + TypeScript + Tailwind CSS

### Admin Panel 页面结构

| 页面路由 | 功能 |
|---------|------|
| `/login` | Login (tokenAuth / OAuth redirect) |
| `/register` | Initial admin registration |
| `/dashboard` | Usage stats overview |
| `/admin/users` | User management + invites |
| `/admin/invitations` | Invitation token management |
| `/admin/models` | Model config UI (completion/chat/embedding) |
| `/admin/repository` | Git repo registration & sync status |
| `/admin/integration` | GitHub/GitLab integration config |
| `/admin/sso` | SSO/OIDC/LDAP configuration |
| `/admin/email` | SMTP settings |
| `/admin/license` | License upload + validation |
| `/admin/access-control` | RBAC + Access Policy management |
| `/admin/analytics` | Usage analytics dashboard |
| `/admin/settings` | Network/branding/security settings |
| `/admin/notifications` | User notification center |
| `/documents` | Custom documents list |
| `/pages` | Web Pages list |
| `/threads` | Chat thread history |

---

## 4.6 ee/tabby-email — Email Templates

**职责**: SMTP 邮件发送 + HTML template rendering。

### Template Types

| Template | 触发场景 |
|----------|---------|
| `invitation.html` | Admin 邀请新用户注册 |
| `password_reset.html` | 用户密码重置链接 |
| `welcome.html` | 新账号欢迎邮件 |

---

## 4.7 Enterprise Features Summary Table

### 功能矩阵

| 功能 | EE Component | API Surface | 配置位置 |
|------|-------------|-------------|---------|
| **用户管理** | tabby-db/users.rs | GraphQL `users`, `register` | Admin Panel → Users |
| **OAuth SSO** | ee/tabby-webserver/oauth/ | GraphQL `authProviders`, Mutation | Admin Panel → SSO |
| **LDAP** | ee/tabby-webserver/ldap.rs | GraphQL setting queries/mutations | Admin Panel → LDAP |
| **RBAC** | access_policy.rs + user_group.rs | GraphQL `accessPolicies`, RBAC API | Admin Panel → Access Control |
| **License 管理** | license.rs + tabby-db | GraphQL `license`, Mutation | Admin Panel → License |
| **Email SMTP** | tabby-email/ | GraphQL `emailSetting` | Admin Panel → Email |
| **Usage Analytics** | analytic.rs | GraphQL `dailyStats`, `userEvents` | Admin Panel → Analytics |
| **Git/GitLab 集成** | integration.rs + third_party jobs | GraphQL `integrations`, Mutation | Admin Panel → Integration |
| **自定义文档摄取** | ingestion.rs + crawler | GraphQL `ingestionStatus`, `webDocuments` | Admin Panel → Documents |
| **Web Pages** | page.rs | GraphQL `pages`, Subscriptions | Web UI → Pages |
| **Chat Threads** | thread.rs + user_chats.rs | GraphQL `threads`, Subscriptions | Chat Panel |
| **通知系统** | notification.rs | GraphQL `notifications` | User Notification Center |
| **Usage Reporting** | daily.rs job + analytic.rs | Reports page in Admin | Admin Panel → Reports |

---

## 4.8 EE Database Schema Overview

### Key Tables (from migrations 0001-0050)

```sql
-- Users & Auth
_users                  -- User accounts (id, name, email, password_hash, role)
_invitations            -- Registration invitations (token, email, created_at)
_refresh_tokens         -- JWT refresh tokens (user_id, token, expires_at)
_oauth_credentials      -- OAuth provider configs (provider, client_id, client_secret)
_ldap_setting           -- LDAP connection config (host, bind_dn, search_base)

-- Repository & Integration
_git_repositories       -- Managed Git repos (id, name, git_url, active)
_integrations           -- GitHub/GitLab integration configs (provider, access_token)
_third_party_sync_status  -- Sync status per repository

-- Access Control
_access_policies        -- RBAC rules (user_id/group_id, resource_type, resource_id, allowed)
_user_groups            -- User groups (id, name)
_user_group_members     -- Group membership (group_id, user_id)

-- Documents & Ingestion
_pages                  -- Web Pages (id, title, content_hash, created_at)
_web_documents          -- Custom docs (id, url, title, type)
_ingestion              -- Ingestion queue/status (url, status, error, created_at)
_commit_indexing_status -- Indexing progress per commit

-- Analytics & Events
_daily_stats            -- Aggregated daily usage stats (date, metric, value)
_user_events            -- Raw IDE events (id, user_id, event_type, payload, created_at)
_analytics_snapshot     -- Team-level analytics snapshots

-- Email & Settings
_email_setting          -- SMTP config (host, port, username, password_encrypted, tls_enabled)
_server_settings        -- Server settings (network/branding/security as JSON blobs)
_license                -- License key + validation result (key_json, is_valid, expires_at)

-- Notifications
_notifications            -- User notifications (id, user_id, type, payload, read_at)

-- Thread & Chat
_threads                  -- Chat threads (id, user_id, title, created_at)
_thread_messages          -- Messages per thread (thread_id, role, content, timestamp)
_user_chats             -- Per-user chat history index
```
