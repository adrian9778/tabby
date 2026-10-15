# 第九章：系统管理

## 9.1 User Management

### User Roles

库里**没有 `role` 列 / `Role` 枚举**：`ee/tabby-db/src/users.rs` · `UserDAO` 存的是 `is_admin: bool`（同表还有 `active`、`auth_token`、`password_encrypted`）。GraphQL 侧 `ee/tabby-schema/src/schema/auth.rs` · `struct UserSecured` +187 暴露 `isAdmin` / `isOwner` / `active` / `isSsoUser` / `isPasswordSet`；改权限的 mutation 是 `updateUserRole(id:, isAdmin:)`。

| 身份 | 权限范围 |
|------|---------|
| `isAdmin = true` | 全量管理权限: users, settings, license, analytics；`check_admin` 放行的 GraphQL resolver |
| `isAdmin = false` | 使用 API / IDE 扩展; 聊天; 查看自己的 events/analytics |
| `isOwner` | 首个注册的管理员（`auth_token` 型访问在 license 失效时只有 owner 还能用，见 `ee/tabby-webserver/src/service/mod.rs` · `authorize_request`） |

### User Operations (GraphQL)

```graphql
# List all users (paginated with Relay cursor)
# ee/tabby-schema/src/schema/mod.rs · users +0
query {
  users(first: 20, after: null) {
    edges {
      node {
        id
        email
        name
        isAdmin     # Boolean —— 不是 role 枚举
        isOwner
        active      # true | false (disabled account)
        isSsoUser
        createdAt
      }
      cursor
    }
    pageInfo { hasNextPage endCursor }
  }
}

# Create invitation link —— 返回值是邀请记录的 ID，不是 token
mutation {
  createInvitation(email: "newuser@example.com")
}

# Enable/disable user（**没有** toggleUserActive）
mutation {
  updateUserActive(id: "user-uuid", active: false)
}

# 改名 / 改权限
mutation { updateUserName(id: "user-uuid", name: "New Name") }
mutation { updateUserRole(id: "user-uuid", isAdmin: true) }

# 注意：**不存在 deleteUser** —— 只能 updateUserActive(id:, active: false) 停用
```

### Registration Flow

| 步骤 | 操作 | API |
|------|------|-----|
| 1. Admin 注册 | 系统未初始化时注册首个管理员 | `POST /graphql` → mutation `register(email:, password1:, password2:, name:)`（**没有** `password:` 单参数，必须给两次密码） |
| 2. Generate token | 读取注册 token（用于确认系统是否已初始化） | Query `registrationToken`；配合 Query `serverInfo` 的 `isAdminInitialized` 判断 |
| 3. Invite users | 生成邀请记录，再邮件发出 | Mutation `createInvitation(email:)`（返回邀请 ID）+ Mutation `requestInvitationEmail(email:)` |
| 4. User signup | 新用户带邀请 code 完成注册 | `POST /graphql` → mutation `register(email:, password1:, password2:, name:, invitationCode:)` |
| 5. 自助注册开关 | 不是「注册页默认关闭」，而是**双条件同时满足才放开**：SMTP 已配置 **且** `securitySetting.allowedRegisterDomainList` 非空 | 见 `ee/tabby-webserver/src/service/auth.rs` · `allow_self_signup` +0；域名列表在 `/settings/general` 里改 |

密码重置走 `generateResetPasswordUrl(id:)` / `requestPasswordResetEmail(email:)` 两个 mutation，落地到 `ee/tabby-ui/app/auth/reset-password` 页面，再用 `passwordReset(input: PasswordResetInput)` 提交（`input` 里是 `code` + `password1` + `password2`，**不是** `token` + `password`）。

---

## 9.2 Authentication & SSO

### Supported Auth Methods

| Method | EE Component | Config Location |
|--------|-------------|-----------------|
| **Password** (built-in) | `ee/tabby-webserver/src/service/auth.rs` | `/settings/team` |
| **GitHub OAuth2** | `ee/tabby-webserver/src/oauth/github.rs` | `/settings/sso` |
| **GitLab OAuth2** | `ee/tabby-webserver/src/oauth/gitlab.rs` | `/settings/sso` |
| **Google OAuth2** | `ee/tabby-webserver/src/oauth/google.rs` | `/settings/sso` |
| **OIDC (OpenID Connect)** | `ee/tabby-webserver/src/oauth/oidc.rs` | `/settings/sso` |
| **LDAP** | `ee/tabby-webserver/src/ldap.rs` | `/settings/sso/detail/ldap` |

四个 provider 与 `mod.rs` 同在 `ee/tabby-webserver/src/oauth/`（`src/` 这一段不能省，`ee/tabby-webserver/oauth/` 这个目录不存在）；OAuth 凭据本身存在 `ee/tabby-db/src/oauth_credential.rs` 的 `oauth_credential` 表，LDAP 凭据存在 `ee/tabby-db/src/ldap_credential.rs` 的 `ldap_credential` 单行表。OIDC 只支持 OpenID Connect，**不涉及 SAML**。

### OAuth / LDAP Configuration (GraphQL)

```graphql
# 配置 GitHub OAuth —— 只有 updateOauthCredential，不存在 createOauthCredential / deleteOauthCredential 中的 create
# 且该 mutation 被 check_license(&[LicenseType::Enterprise]) 硬门控
mutation {
  updateOauthCredential(input: {
    provider: GITHUB
    clientId: "your-github-oauth-client-id"
    clientSecret: "your-github-oauth-secret"
    configUrl: "https://tabby.yourcompany.com"   # 可选：自定义 authorize/token endpoint
    configScopes: "read:user user:email"         # 可选
  })
}

# 查询回调地址（配 OAuth 应用时用这个，别手拼）
query { oauthCallbackUrl(provider: GITHUB) }

# Enable LDAP —— mutation 叫 updateLdapCredential，input 是 UpdateLdapCredentialInput
# 同样需要 Enterprise license；配套还有 testLdapConnection / deleteLdapCredential / tokenAuthLdap
mutation {
  updateLdapCredential(input: {
    host: "ldap.yourcompany.com"
    port: 389
    bindDn: "cn=admin,dc=yourcompany,dc=com"
    bindPassword: "ldap-admin-password"
    baseDn: "ou=users,dc=yourcompany,dc=com"      # 不是 searchBase
    userFilter: "(uid=%s)"                        # 不是 searchFilter，且要求括号完整
    encryption: NONE                              # LdapEncryptionKind
    skipTlsVerify: false
    emailAttribute: "mail"                        # 必填
    nameAttribute: "cn"                           # 可选
  })
}
```

字段定义见 `ee/tabby-schema/src/schema/auth.rs` · `struct UpdateOAuthCredentialInput` +386 / `struct UpdateLdapCredentialInput` +412；LDAP 登录走 `tokenAuthLdap(userId:, password:)`。

### JWT Token Lifecycle

| 事件 | TTL | Refresh |
|------|-----|---------|
| Access Token (JWT) | **30 分钟**（`Module:ee/tabby-webserver/src/jwt.rs` · `JWT_DEFAULT_EXP = 30 * 60`） | Mutation `refreshToken` |
| Refresh Token | **7 天**（`ee/tabby-db/src/refresh_tokens.rs` · `create_refresh_token` +0，SQL 里写死 `datetime('now', '+7 days')`） | 存 `refresh_tokens` 表，`renew_refresh_token` 顺延 |

JWT 密钥来自 `TABBY_WEBSERVER_JWT_TOKEN_SECRET`，未设置时进程内随机生成（重启即失效）；过期清理由同文件 · `delete_expired_token` +0 负责。

---

## 9.3 Access Control & RBAC

### Access Policy Model

真实的授权维度**只有一个**：某个 `source_id`（代码 / 文档 / Page 的检索源标识）对某个**用户组**可读。

```
UserGroup ──grantSourceIdReadAccess──▶ source_id（Repository / IngestedDoc / Page 的检索源）
    ▲                                        │
    │            revokeSourceIdReadAccess    ▼
    └────────────────────────────────────────┘

User ──upsertUserGroupMembership(isGroupAdmin)──▶ UserGroup
```

没有 `resource_type` / `resource_id` / `allowed` 这三样东西，也**不存在** `grantAccessPolicy` / `revokeAccessPolicy` / `accessPolicies` 这一组 API —— 它们对应的真实名字是 `grantSourceIdReadAccess` / `revokeSourceIdReadAccess` / `sourceIdAccessPolicies`（定义在 `ee/tabby-schema/src/schema/mod.rs` 的 Mutation / Query impl 里，服务实现 `ee/tabby-webserver/src/service/access_policy.rs`）。

### Key Tables

| Table | Columns |
|-------|---------|
| `source_id_read_access_policies` | id, source_id, user_group_id, created_at, updated_at（`(source_id, user_group_id)` 唯一） |
| `user_groups` | id, name, created_at, updated_at（**没有** description） |
| `user_group_memberships` | id, user_id, user_group_id, is_group_admin, created_at, updated_at |

三张表都由 `ee/tabby-db/migrations/0038_add-access-control-tables.up.sql` 建，DAO 在 `ee/tabby-db/src/access_policy.rs` 与 `ee/tabby-db/src/user_groups.rs`。

### RBAC Operations

```graphql
# Create user group —— 入参是 input 对象；name 只能小写字母开头、2~20 字符
mutation {
  createUserGroup(input: { name: "engineering" })
}

# Add / update member（**没有** addUserToGroup）
mutation {
  upsertUserGroupMembership(input: {
    userGroupId: "<group-id>"
    userId: "<user-id>"
    isGroupAdmin: false
  })
}

mutation { deleteUserGroupMembership(userGroupId: "<group-id>", userId: "<user-id>") }

# Grant read access on one source id to one group
mutation {
  grantSourceIdReadAccess(sourceId: "git:1", userGroupId: "<group-id>")
}

# List which groups can read a given source id —— 不是 Relay connection，直接返回对象
query {
  sourceIdAccessPolicies(sourceId: "git:1") {
    sourceId
    read {
      id
      name
      members { user { name email } isGroupAdmin }
    }
  }
}

query { userGroups { id name createdAt members { user { id } isGroupAdmin } } }
```

---

## 9.4 License Management

### License Tiers

档位就是 `ee/tabby-schema/src/schema/license.rs` · `enum LicenseType` 的三个变体，**没有 Free / Pro**：

| Tier | 席位 | 管理员数 | 说明 |
|------|------|---------|------|
| **Community** | `min(seats, 5)` | 最多 1 | 未上传 license 时 `LicenseService::read` 返回的默认档 |
| **Team** | `min(seats, 50)` | 最多 3 | 需有效 license |
| **Enterprise** | license 里的 `seats` 原值 | 不限 | 唯一被 `check_license` 硬门控的档：`updateOauthCredential` `testLdapConnection` `updateLdapCredential` `updateSecuritySetting` |

另有 `enum LicenseStatus`（`Ok` / `Expired` / `SeatsExceeded`）与 `enum LicenseFeature`（目前只有 `CustomLogo`）。

### License Operations

license **是一串 JWT**，不是 PEM 文件：`ee/tabby-webserver/src/service/license.rs` · `validate_license` 用 `RS512` + `Algorithm` 校验，公钥来自仓库内 `ee/tabby-webserver/keys/license.key.pub`，并要求 `iss == "tabbyml.com"`、必填 claims `exp/iat/sub/iss`。

```graphql
# 上传 license（入参是 JWT 字符串；mutation 只回 true/false）
mutation { uploadLicense(license: "<jwt-token>") }

# 清空 license，回落 Community
mutation { resetLicense }

# Check current license —— 字段名以 LicenseInfo 为准
query {
  license {
    type            # LicenseType 枚举：COMMUNITY | TEAM | ENTERPRISE（不是 tier）
    status          # Ok | Expired | SeatsExceeded（不是 valid 布尔）
    seats           # 不是 maxUsers
    seatsUsed
    issuedAt
    expiresAt
    features        # [LicenseFeature]，目前只有 CUSTOM_LOGO
  }
}
```

`uploadLicense` 会先算 `count_active_users()`，状态不是 `Ok` 就直接 `bail!`（"License is expired" / "License doesn't contain sufficient number of seats"），**不会写库**；demo 模式（`is_demo_mode()`）下禁止修改 license。

### License Validation Job

- **频率**: **Daily**，不是 Hourly。链路是 `ee/tabby-webserver/src/service/background_job/mod.rs` · `start` 里的 `@daily` `CronStream` → `BackgroundJobEvent::Daily` → `daily.rs` · `DailyJob::run` → `LicenseCheckJob::cron`（`license_check.rs` · `LicenseCheckJob` 的 `NAME = "license_check"`）
- **动作**: **完全离线**——`license_service.read()` 解析本地存的 JWT 并比对 `expires_at`；代码里没有任何访问 TabbyML 激活服务器的 HTTP 调用
- **行为**: `Community` 档直接 `return Ok(())` 跳过；其余档在 `0 < expire_in_days < 7` 时调用 `notification_service.create(NotificationRecipient::Admin, ...)` 给管理员发一条到期提醒
- **过期后果**: 不是「自动降级到 Free」。`status` 变成 `Expired` 后 `ensure_valid_license` 让需要 license 的操作报 `CoreError::InvalidLicense`；`serverInfo` 也会因 license 无效而只允许 owner 的 auth token 访问 `/v1/*`（`ee/tabby-webserver/src/service/mod.rs` · `authorize_request`）

---

## 9.5 Custom Documents & Ingestion

### Document Types

| Type | Source | 索引方式 |
|------|--------|---------|
| **Preset / Custom Web Documents** | URL → `WebCrawlerJob` 抓取（`crates/tabby-crawler/`，含 `llms_txt_parser.rs`） | 爬完直接写 Tantivy |
| **Ingested Documents**（REST 推送正文） | `POST /v1beta/ingestion`（`ee/tabby-webserver/src/routes/ingestion.rs`） | 先落 `ingested_documents` 表，再由 `SyncIngestionIndexJob` 索引 |
| **Web Pages** | 由会话 / `createPageRun` 生成，页面自身也可挂 source_id 被爬 | `IndexPagesJob` |
| **GitHub/GitLab PRs & Issues** | Third-party integration | `third_party_integration` 下的 pulls / issues 同步任务 |

### Ingestion Process

两条不同的链路，不要混：

```
(A) 文档推送（ingestion）—— 正文由调用方给，服务器不抓取
POST /v1beta/ingestion  IngestionRequest { source, id, title, body, link, ttl }
    → 写入 ingested_documents 表，status = PENDING
    → CronStream "*/10 * * * * *"（每 10 秒）调度 SyncIngestionIndexJob
      （ee/tabby-webserver/src/service/background_job/index_ingestion.rs，NAME = "ingestion_index_sync"）
    → tabby_index 的 StructuredDocIndexer 写 Tantivy
    → status 更新为 INDEXED（失败则 FAILED）

(B) URL 抓取（web crawler）—— 只给 URL
createCustomDocument(name:, url:) / 启用 preset 文档
    → ee/tabby-webserver/src/service/web_documents.rs · create_custom_web_document
    → job_service.trigger(BackgroundJobEvent::WebCrawler(WebCrawlerJob::new(source_id, url, url_prefix)))
    → web_crawler.rs 调 tabby_crawler::crawl_pipeline（超时 CRAWLER_TIMEOUT_SECS = 7200）
    → readability 抽取 + llms.txt 解析 → StructuredDocIndexer 写 Tantivy
```

status 取值只有 `PENDING` / `INDEXED` / `FAILED` 三种（`ee/tabby-db/src/ingestion.rs` · `enum IngestedDocumentStatusDAO` +24），**没有** `processing` / `done` / `error`。

### Ingestion Management (GraphQL)

```graphql
# 注册一个待抓取的自定义文档（mutation 名是 createCustomDocument，不是 createWebDocument）
mutation {
  createCustomDocument(input: { name: "api-docs", url: "https://docs.example.com/api" })
}

# 抓取进度按 source 聚合，返回 Vec，不是单对象
query {
  # source id 由 schema 层拼出来：custom_web_document:{id} / preset_web_document:{name}
  # （ee/tabby-schema/src/schema/web_documents.rs · format_source_id +19）
  ingestionStatus(sources: ["custom_web_document:1"]) {
    source
    pending      # 三个计数：
    failed
    total
  }
}

# 删除文档（同时移除索引）
mutation { deleteCustomDocument(id: "<doc-id>") }

# preset 文档只允许启停，不允许改内容
mutation { setPresetDocumentActive(input: { id: "<id>", active: true }) }
```

对应的表是 `web_documents`（preset/custom 用 `is_preset` 区分）与 `ingested_documents`，DAO 在 `ee/tabby-db/src/web_documents.rs` 和 `ee/tabby-db/src/ingestion.rs`；服务在 `ee/tabby-webserver/src/service/web_documents.rs` 与 `ee/tabby-webserver/src/service/ingestion.rs`。

---

## 9.6 Code Repository Integration

### Managed Repositories (EE)

Git repositories registered for indexing and context retrieval. 存的是 `repositories` 表（`ee/tabby-db/src/repositories.rs` · `RepositoryDAO`），服务在 `ee/tabby-webserver/src/service/repository/git.rs`；界面上在 `/settings/providers/git`。

```graphql
# Add a Git repository —— 返回新建记录的 ID，不是对象
mutation {
  createGitRepository(
    name: "my-project",
    gitUrl: "https://github.com/org/my-project.git"
    refs: ["+refs/heads/main:refs/remotes/origin/main"]   # 可选，0050 迁移加的列
  )
}

mutation { updateGitRepository(input: UpdateGitRepositoryInput) }
mutation { deleteGitRepository(id: "<id>") }

# List repositories —— node 是 GitRepository：id / name / gitUrl / refs / jobInfo
# **没有** active、也**没有** lastIndexedAt（active 属于 provided_repositories）
query {
  gitRepositories(first: 20) {
    edges {
      node {
        id
        name
        gitUrl
        refs { name commit }
        jobInfo {
          command          # 可直接喂给 triggerJobRun 的 JSON 字符串
          lastJobRun { id job exitCode startedAt finishedAt }
        }
      }
    }
    pageInfo { hasNextPage endCursor }
  }
}
```

`GitRepository` 的字段定义见 `ee/tabby-schema/src/schema/repository/git.rs` · `struct GitRepository` +26；仓库的检索源 id 统一是 `git:{id}`（同文件 · `format_source_id` +11）。

### Third-Party Integration (GitHub / GitLab)

Synchronizes issues, pull requests, and merge requests into the indexing pipeline:

```graphql
# Create GitHub integration —— 入参是 input 对象，字段是 kind / displayName / accessToken / apiBase
# IntegrationKind 只有 GITHUB / GITLAB / GITHUB_SELF_HOSTED / GITLAB_SELF_HOSTED
# 返回值是新建记录的 ID（不是对象，也没有 syncStatus 字段）
mutation {
  createIntegration(input: {
    kind: GITHUB
    displayName: "org/repo"
    accessToken: "ghp_xxxxxx"
    apiBase: null            # 自建实例才需要填
  })
}

mutation { updateIntegration(input: { ... }) }
mutation { deleteIntegration(id: "<id>", kind: GITHUB) }

# Trigger sync manually —— command 是 BackgroundJobEvent 的 serde JSON（externally tagged）
# **不存在** jobType: TRIGGER_REPO_SYNC 这种写法
mutation {
  triggerJobRun(command: "{\"SyncThirdPartyRepositories\":1}")
}

# 可用的 job 名（query jobs 直接列出来）
query { jobs }
```

`command` 的合法形状见 `ee/tabby-webserver/src/service/background_job/mod.rs` · `enum BackgroundJobEvent` +86（`SchedulerGitRepository` / `SchedulerGithubGitlabRepository` / `SyncThirdPartyRepositories` / `WebCrawler` / `IndexGarbageCollection` / `SyncIngestionIndex` / `SyncPagesIndex` / `Hourly` / `Daily`），`to_command` 就是把该枚举 `serde_json::to_string`。

集成记录本身存 `integrations` 表（`ee/tabby-db/src/integrations.rs` · `IntegrationDAO`：`kind` / `display_name` / `access_token` / `api_base` / `error` / `synced`），同步下来的仓库存 `provided_repositories`（`ee/tabby-db/src/provided_repositories.rs`，**这张表才有 `active`**）。

### Indexed Content from Integrations

| 源类型 | 索引字段 | 用途 |
|--------|---------|------|
| Pull Request | title, body, diff, files | Code completion context (code chunks) |
| GitHub Issue / GitLab MR | title, description, comments | Answer Engine RAG context |
| Commit messages + diffs | message, added_lines, removed_lines | Code search across history |

---

## 9.7 Email Settings (SMTP)

### SMTP Configuration

```graphql
# Configure SMTP server
mutation {
  updateEmailSetting(
    host: "smtp.yourcompany.com",
    port: 587,
    username: "noreply@yourcompany.com",
    password: "smtp-password",
    tlsEnabled: true      # Use STARTTLS
  ) {
    success
  }
}

# Test email delivery
mutation {
  triggerTestEmail(to: "admin@example.com") {
    sent
    error
  }
}
```

### Email Templates (in `ee/tabby-email/`)

| Template | Trigger | Variables |
|----------|---------|-----------|
| `invitation.html` | New user invited | `name`, `inviteUrl`, `serverName` |
| `password_reset.html` | User requests password reset | `name`, `resetUrl`, `expiry` |
| `welcome.html` | First login (optional) | `name`, `serverUrl` |

---

## 9.8 Server Settings

### Setting Categories

```graphql
# Network settings (host, port, HTTPS)
query { networkSetting { host port https } }
mutation { updateNetworkSetting(host: "0.0.0.0", port: 8080, https: true) { success } }

# Security settings (registration, CORS, rate limits)
query { securitySetting { allowRegistration corsAllowOrigins } }
mutation { updateSecuritySetting(allowRegistration: false) { success } }

# Branding settings (logo, colors, custom CSS)
query { brandingSetting { logoUrl primaryColor customCss } }
mutation { updateBrandingSetting(logUrl: "https://...", customCss: "...") { success } }
```

### Settings Persistence

All settings are stored as JSON blobs in the `_server_settings` table in SQLite:

```sql
INSERT OR REPLACE INTO _server_settings (key, value)
VALUES ('network', '{"host":"0.0.0.0","port":8080}');
```

---

## 9.9 Usage Analytics

### Metrics Available

| Metric | Aggregation | Query API |
|--------|------------|-----------|
| **Completion** (view/select) | Daily count per user | GraphQL `dailyStats` |
| **Chat** (view/select) | Daily count per user | GraphQL `dailyStats` |
| **Event detail** | Raw event log | GraphQL `userEvents` |
| **Disk Usage** | Storage stats | GraphQL `diskUsageStats` |
| **Team Analytics** | Aggregated summaries | Reports page in Admin UI |

### GraphQL Query Example

```graphql
# Get usage stats for a user in a date range
query {
  dailyStats(
    user: "user-uuid",
    since: "2025-07-01",
    until: "2025-07-31"
  ) {
    completion { view select }    # {"view": 1234, "select": 567}
    chat { view select }          # {"view": 89, "select": 34}
  }
}

# Get team-wide stats (all users)
query {
  dailyStats(
    since: "2025-07-01",
    until: "2025-07-31"
  ) {
    completion { view select }
    chat { view select }
  }
}

# Raw user events (for detailed analysis)
query {
  userEvents(
    first: 100,
    after: null,
    filter: { type: SELECT, since: "2025-07-01" }
  ) {
    edges {
      node {
        id
        userId
        type       # "view" | "select" | "completion_time"
        payload    # JSON details
        createdAt
      }
    }
  }
}
```

---

## 9.10 Web Pages & Threads

### Web Pages (Persistent Answer Engine Content)

Web Pages are persistent content pages created from Answer Engine interactions:

```graphql
# Create a new page
mutation {
  createPage(title: "Project Architecture Guide", markdown: "# ...") {
    id
    title
    url
  }
}

# Get all pages for a user
query {
  pages(first: 20) {
    edges {
      node {
        id
        title
        createdAt
        updatedAt
      }
    }
  }
}

# Stream page generation (real-time)
subscription {
  createPageRun(input: { prompt: "Summarize our codebase architecture" }) {
    delta
    done
  }
}
```

### Chat Threads

```graphql
# List user's threads
query {
  threads(first: 50, after: null) {
    edges {
      node {
        id
        title
        messageCount
        createdAt
        updatedAt
      }
    }
  }
}

# Stream a new thread run
subscription {
  createThreadRun(threadId: "thread-uuid", message: "How does auth work here?") {
    delta       # Streaming text chunk
    done        # true when complete
  }
}
```

---

## 9.11 Notifications

### Notification Types

| Type | Trigger | Destination |
|------|---------|------------|
| `job_complete` | Background job finished | UI notification bell + email [EE] |
| `job_error` | Background job failed | UI + email |
| `invitation_sent` | User invited | Email (sent by SMTP) |
| `license_expiring` | License < 7 days from expiry | Admin notification |

### Notification Operations

```graphql
# Get unread notifications
query {
  notifications(filter: { read: false }, first: 50) {
    edges {
      node {
        id
        type
        payload
        createdAt
      }
    }
  }
}

# Mark as read
mutation {
  markNotificationRead(id: "notif-uuid") { success }
}
```

---

## 9.12 Admin Operations Summary Table

| 管理任务 | 入口 | API（Q = query，其余为 mutation；名称全部取自 `ee/tabby-schema/src/schema/mod.rs`） |
|---------|------|-----|
| 用户管理 | Admin → Users | Q `users`; `register`, `createInvitation`, `deleteInvitation`, `updateUserActive`, `updateUserRole`, `resetUserAuthToken`, `logoutAllSessions` |
| SSO/OAuth 配置 | Admin → SSO | Q `authProviders`, `oauthCredential`, `oauthCallbackUrl`; `updateOauthCredential`, `deleteOauthCredential` |
| LDAP 配置 | Admin → SSO | Q `ldapCredential`; `updateLdapCredential`, `deleteLdapCredential`, `testLdapConnection`, `tokenAuthLdap` |
| 用户组 / 访问策略 | Admin → Access Control | Q `userGroups`, `sourceIdAccessPolicies`; `createUserGroup`, `deleteUserGroup`, `upsertUserGroupMembership`, `deleteUserGroupMembership`, `grantSourceIdReadAccess`, `revokeSourceIdReadAccess` |
| License 管理 | Admin → License | Q `license`; `uploadLicense`, `resetLicense` |
| Email/SMTP | Admin → Email | Q `emailSetting`; `updateEmailSetting`, `deleteEmailSetting`, `sendTestEmail` |
| 安全 / 网络 / 品牌设置 | Admin → Settings | Q `securitySetting`, `networkSetting`, `brandingSetting`; `updateSecuritySetting`, `updateNetworkSetting`, `updateBrandingSetting` |
| 模型配置 | 改 `~/.tabby/config.toml` | **GraphQL 里没有任何模型增删改操作**，只有一个连通性测试查询 `testModelConnection` |
| Git Repos | Admin → Repositories | Q `gitRepositories`; `createGitRepository`, `updateGitRepository`, `deleteGitRepository`, `triggerJobRun` |
| 集成管理 | Admin → Integration | Q `integrations`, `integratedRepositories`; `createIntegration`, `updateIntegration`, `deleteIntegration`, `updateIntegratedRepositoryActive`, `updateIntegratedRepositoryRefs` |
| 自定义文档 | Admin → Documents | Q `customWebDocuments`, `presetWebDocuments`; `createCustomDocument`, `deleteCustomDocument`, `setPresetDocumentActive` |
| Usage Analytics | Admin → Analytics | GraphQL `dailyStats`, `userEvents` |
| Web Pages | Web UI → Pages | GraphQL `pages`, `createPage` |
| Chat Threads | Chat Panel | GraphQL `threads`, subscriptions |
| 服务器设置 | Admin → Settings | GraphQL `networkSetting/securitySetting/brandingSetting` |
| Notifications | Notification Bell | GraphQL `notifications`, `markNotificationRead` |
