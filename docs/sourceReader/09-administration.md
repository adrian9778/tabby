# 第九章：系统管理

## 9.1 User Management

### User Roles

| Role | 权限范围 |
|------|---------|
| `admin` | 全量管理权限: users, models, settings, license, analytics |
| `user` | 使用 API / IDE 扩展; 聊天; 查看自己的 events/analytics |

### User Operations (GraphQL)

```graphql
# List all users (paginated with Relay cursor)
query {
  users(first: 20, after: null) {
    edges {
      node {
        id
        name
        email
        role     # "admin" | "user"
        active   # true | false (disabled account)
        createdAt
      }
      cursor
    }
    pageInfo { hasNextPage endCursor }
  }
}

# Create invitation link
mutation {
  createInvitation(email: "newuser@example.com") {
    token          # Invitation token for signup
    email
    createdAt
  }
}

# Enable/disable user
mutation {
  toggleUserActive(id: "user-uuid", active: false) {
    id
    active
  }
}

# Delete user
mutation {
  deleteUser(id: "user-uuid") { success }
}
```

### Registration Flow

| 步骤 | 操作 | API |
|------|------|-----|
| 1. Admin 注册 | Initial admin signup (only once per server) | `POST /graphql` → mutation `register` |
| 2. Generate token | Get registration token for inviting others | Query `registrationToken` |
| 3. Invite users | Send invitation link or raw token to new user | Mutation `createInvitation(email:)` |
| 4. User signup | New user completes registration with token | `POST /graphql` → mutation `register(name:, email:, password:, token:)` |
| 5. Post-registration | Registration page auto-closes for all future users (closed by default) | Admin can re-open via settings |

---

## 9.2 Authentication & SSO

### Supported Auth Methods

| Method | EE Component | Config Location |
|--------|-------------|-----------------|
| **Password** (built-in) | `service/auth.rs` | Admin Panel → Users |
| **GitHub OAuth2** | `ee/tabby-webserver/oauth/github.rs` | Admin Panel → SSO |
| **GitLab OAuth2** | `ee/tabby-webserver/oauth/gitlab.rs` | Admin Panel → SSO |
| **Google OAuth2** | `ee/tabby-webserver/oauth/google.rs` | Admin Panel → SSO |
| **OIDC (SAML/SAML-like)** | `ee/tabby-webserver/oauth/oidc.rs` | Admin Panel → SSO |
| **LDAP** | `ee/tabby-webserver/ldap.rs` | Admin Panel → LDAP |

### OAuth Configuration (GraphQL)

```graphql
# Create GitHub OAuth provider
mutation {
  createOauthCredential(
    provider: GITHUB,
    clientId: "your-github-oauth-client-id",
    clientSecret: "your-github-oauth-secret",
    appUrl: "https://tabby.yourcompany.com"
  ) {
    id
    provider
    clientId
  }
}

# Enable LDAP
mutation {
  updateLdapSetting(
    host: "ldap.yourcompany.com",
    port: 389,
    bindDn: "cn=admin,dc=yourcompany,dc=com",
    bindDnPassword: "ldap-admin-password",
    searchBase: "ou=users,dc=yourcompany,dc=com",
    searchFilter: "uid=%s"
  ) {
    success
  }
}
```

### JWT Token Lifecycle

| 事件 | TTL | Refresh |
|------|-----|---------|
| Access Token (JWT) | 24 hours | Mutation `refreshToken` |
| Refresh Token | 30 days | Stored in SQLite `_refresh_tokens` table |

---

## 9.3 Access Control & RBAC

### Access Policy Model

```
User / UserGroup ──grant──▶ AccessPolicy ──on──▶ Resource (Repository/Page/Setting/etc)
         ▲                              │
         │          revoke              ▼
         └───────────────────── revocation ─────────┘
```

### Key Tables

| Table | Columns |
|-------|---------|
| `_access_policies` | user_id, group_id, resource_type, resource_id, allowed (bool) |
| `_user_groups` | id, name, description |
| `_user_group_members` | group_id, user_id |

### RBAC Operations

```graphql
# Create user group
mutation {
  createUserGroup(name: "engineering") {
    id
    name
  }
}

# Add member to group
mutation {
  addUserToGroup(groupId: "group-uuid", userId: "user-uuid") { success }
}

# Grant access policy
mutation {
  grantAccessPolicy(
    groupId: "group-uuid",
    resourceType: REPOSITORY,
    resourceId: "repo-uuid",
    allowed: true
  ) { id }
}

# List all access policies for a repository
query {
  accessPolicies(resourceType: REPOSITORY, resourceId: "repo-uuid") {
    edges {
      node {
        user { name email }
        group { name }
        allowed
      }
    }
  }
}
```

---

## 9.4 License Management

### License Tiers

| Tier | Features Included | Validation |
|------|-----------------|------------|
| **Free** | Core REST API only | No license needed |
| **Pro** | EE GraphQL + RBAC + Analytics | Valid Pro license key |
| **Enterprise** | All features + LDAP + Custom Docs | Valid Enterprise license key |

### License Operations

```graphql
# Upload a license file (PEM-encoded)
mutation {
  updateLicense(key: "license-pem-content") {
    valid
    tier            # "free" | "pro" | "enterprise"
    maxUsers
    expiresAt
  }
}

# Check current license
query {
  license {
    valid
    tier
    maxUsers
    expiresAt
  }
}
```

### License Validation Job

- **频率**: Hourly (background job `license_check.rs`)
- **动作**: Server periodically validates license with TabbyML activation server
- **Fail behavior**: If license expires, falls back to Free tier (EE features disabled)

---

## 9.5 Custom Documents & Ingestion

### Document Types

| Type | Source | 索引方式 |
|------|--------|---------|
| **Web Pages** | URL crawl → Tantivy | Crawler + llms.txt parser |
| **Custom Documents** | Direct upload / API ingestion | Direct indexing |
| **GitHub/GitLab PRs & Issues** | Third-party integration | Auto-sync background job |

### Ingestion Process

```
URL submitted (Admin UI or API)
    → Created in _ingestion table with status "pending"
    → Cron webhook_crawler picks up the URL
    → HTTP fetch + readability extraction
    → Text normalized → Tantivy index writer
    → Status updated to "done"
```

### Ingestion Management (GraphQL)

```graphql
# Create custom document from URL
mutation {
  createWebDocument(url: "https://docs.example.com/api", name: "API Docs") {
    id
    url
    status      # "pending" | "processing" | "done" | "error"
  }
}

# Check ingestion status
query {
  ingestionStatus(url: "https://docs.example.com/api") {
    status
    error       # Error message if status == "error"
    indexedAt
  }
}

# Delete document (removes from index)
mutation {
  deleteWebDocument(id: "doc-uuid") { success }
}
```

---

## 9.6 Code Repository Integration

### Managed Repositories (EE)

Git repositories registered in Admin Panel for indexing and context retrieval:

```graphql
# Add a Git repository
mutation {
  createGitRepository(
    name: "my-project",
    gitUrl: "https://github.com/org/my-project.git"
  ) {
    id
    name
    gitUrl
    active
  }
}

# List repositories
query {
  gitRepositories {
    edges {
      node {
        id
        name
        gitUrl
        active
        lastIndexedAt
      }
    }
  }
}
```

### Third-Party Integration (GitHub / GitLab)

Synchronizes issues, pull requests, and merge requests into the indexing pipeline:

```graphql
# Create GitHub integration
mutation {
  createIntegration(
    provider: GITHUB,
    token: "ghp_xxxxxx",
    repositoryUrl: "https://github.com/org/repo"
  ) {
    id
    provider
    syncStatus
  }
}

# Trigger sync manually
mutation {
  triggerJobRun(jobType: TRIGGER_REPO_SYNC, repoId: "repo-uuid") {
    success
  }
}
```

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

| 管理任务 | 入口 | API |
|---------|------|-----|
| 用户管理 | Admin → Users | GraphQL `users`, `register`, `createInvitation` |
| SSO/OAuth 配置 | Admin → SSO | GraphQL `authProviders`, `createOauthCredential` |
| LDAP 配置 | Admin → LDAP | GraphQL `updateLdapSetting` |
| RBAC / Access Policy | Admin → Access Control | GraphQL `accessPolicies`, `grantAccessPolicy` |
| License 管理 | Admin → License | GraphQL `license`, `updateLicense` |
| Email/SMTP | Admin → Email | GraphQL `emailSetting`, `triggerTestEmail` |
| 模型配置 | Admin → Models | GraphQL `modelConfig` mutations |
| Git Repos | Admin → Repositories | GraphQL `gitRepositories`, `createGitRepository` |
| 集成管理 | Admin → Integration | GraphQL `integrations`, `createIntegration` |
| 自定义文档 | Admin → Documents | GraphQL `webDocuments`, `createWebDocument` |
| Usage Analytics | Admin → Analytics | GraphQL `dailyStats`, `userEvents` |
| Web Pages | Web UI → Pages | GraphQL `pages`, `createPage` |
| Chat Threads | Chat Panel | GraphQL `threads`, subscriptions |
| 服务器设置 | Admin → Settings | GraphQL `networkSetting/securitySetting/brandingSetting` |
| Notifications | Notification Bell | GraphQL `notifications`, `markNotificationRead` |
