# 第十四章：FAQ & 故障排查

## 14.1 Installation FAQs

### Q: GPU not detected on Docker start?

**A**: 检查 nvidia-container-toolkit 是否正确安装：

```bash
docker run --rm --gpus all nvidia/cuda:12.2-base nvidia-smi
```

如果报错 `NVIDIA-SMI has failed`，重新安装驱动：

```bash
# Ubuntu/Debian
sudo apt install -y nvidia-container-toolkit
sudo systemctl restart docker

# Verify
docker run --rm --gpus all nvidia/cuda:12.2-base nvidia-smi
```

### Q: Docker image pull fails?

**A**: 网络问题，按优先级换源：

```bash
# 1) 官方自建 registry（website/docs/quick-start/installation/*.mdx 里统一用它）
docker pull registry.tabbyml.com/tabbyml/tabby

# 2) GHCR（docker.yml 同时推 ghcr.io/tabbyml/tabby）
docker pull ghcr.io/tabbyml/tabby

# 3) 通用加速器：把你的镜像加速前缀拼在原镜像名前面（示例写法，前缀取决于你所在网络提供的加速器）
docker pull <your-mirror-prefix>/tabbyml/tabby
```

> 注意：仓库里**没有**任何证据表明 TabbyML 在阿里云 / 腾讯云等公共云 registry 上发布了官方镜像，因此不要把某个具体云厂商域名当官方源写进部署脚本。

### Q: Model download hangs or fails?

**A**: 模型文件可能很大（数 GB），检查网络和本地磁盘空间：

```bash
# Check available disk space
df -h ~/.tabby/models/

# 换下载源：只有这两个环境变量（见 crates/tabby-download/src/lib.rs · download_host()）
#   TABBY_DOWNLOAD_HOST=huggingface.co        （默认值，直接改写 host）
#   TABBY_HUGGINGFACE_HOST_OVERRIDE=...       （覆盖 huggingface host）
# 注意：**没有** TABBY_MODEL_DOWNLOAD_TIMEOUT 这个变量，超时不可配。
export TABBY_DOWNLOAD_HOST=hf-mirror.com

# Manual download：目录形态是 {models_dir}/{Registry}/{Name}/ggml，入口文件以 model-00001-of- 开头
mkdir -p ~/.tabby/models/TabbyML/Qwen2.5-Coder-7B/ggml
curl -L https://huggingface.co/... > ~/.tabby/models/TabbyML/Qwen2.5-Coder-7B/ggml/model-00001-of-00001.gguf

# Verify tabby.json exists（放在 {Name} 这一层，不在 ggml/ 里）
echo '{"prompt_template":"<PRE>{prefix}<SUF>{suffix}<MID>"}' > ~/.tabby/models/TabbyML/Qwen2.5-Coder-7B/tabby.json
```

### Q: Cannot access the web UI?

**A**: 检查端口绑定和防火墙：

```bash
# Verify server is running
curl http://localhost:8080/v1/health

# Check what ports are listening
ss -tlnp | grep 8080

# If using Docker, verify port mapping
docker ps --format "table {{.Ports}}" | grep 8080

# Check firewall
sudo ufw status   # Ubuntu
lsof -i :8080     # macOS
```

---

## 14.2 Model Loading Issues

### Q: "Model loading failed" error in `/v1/health`?

**A**: Common causes and fixes：

| Error Message | Cause | Fix |
|--------------|-------|-----|
| `GGUF format invalid` | Corrupted GGUF file | Re-download the model |
| `Cannot allocate memory` | RAM insufficient | Use lower quantization (Q4 → Q3) or larger machine |
| `CUDA error: out of memory` | VRAM too small for model | Reduce `num_gpu_layers`, use CPU, or load smaller model |
| `Model file not found` | Wrong model_id or missing ggml dir | Check `~/.tabby/models/{owner}_{name}/ggml/` exists |

### Q: GPU memory usage is higher than expected?

**A**: Reduce KV cache size by limiting context window：

```toml
# config.toml
[model.completion.local]
context_size = 2048        # Default is 4096, reduce to 2048 or 1024
num_gpu_layers = 32         # Only offload first N layers to GPU (for large models)
```

### Q: Slow inference on RTX 3060 / consumer GPU?

**A**: Try these optimizations：

```toml
[model.completion.local]
parallelism = 8              # Increase CPU threads (match your core count)
enable_fast_attention = true # Enable fast attention if supported by model
num_gpu_layers = 9999        # Push all layers to GPU for max speed
```

### Q: Apple Silicon Metal inference is slow?

**A**: Metal inference on M1/M2 typically runs at ~15-25 tokens/s for 7B models. For faster performance：

- Use a smaller model (1.5B or 3B)
- Enable `enable_fast_attention = true`
- Reduce `context_size` to the minimum needed

---

## 14.3 Completion Quality Issues

### Q: Suggestions are irrelevant or low-quality?

**A**: Check context retrieval and prompt config：

```bash
# Verify code index is up to date
curl http://localhost:8080/v1/health | jq .state   # should be "ready"

# Increase code search relevance
[answer]
code_search_params.min_embedding_score = 0.7     # Higher threshold (default: 0.5)
code_search_params.num_to_return = 20            # More context candidates
```

### Q: Suggestions break after I edit the file?

**A**: This is expected — completion only works in the current buffer's scope. Tabby-agent automatically re-indexes on save, but there may be a 1-3 second delay.

### Q: How to customize postprocessing rules?

**A**: Postprocessors are defined in `clients/tabby-agent/src/codeCompletion/postprocess/`. To modify behavior：

```bash
# Edit the specific processor file
vi clients/tabby-agent/src/codeCompletion/postprocess/trimSpace.ts
# Adjust the threshold or rule logic, then rebuild:
cd clients/tabby-agent && pnpm build
```

---

## 14.4 IDE Extension Issues

### Q: VS Code extension not showing suggestions?

**A**: Troubleshooting steps：

1. **Check server connectivity**:
   ```bash
   curl http://localhost:8080/v1/health
   ```

2. **Check extension status bar**: VS Code status bar should show "Tabby: Connected" or similar

3. **Toggle autocomplete**: `Ctrl+Shift+P` → "Tabby: Enable Autocomplete"

4. **Check server URL in settings**:
   - Open Settings (Ctrl+,) → Search "tabby" → verify `server.url`

5. **Restart the extension**: `Ctrl+Shift+P` → "Developer: Reload Window"

### Q: IntelliJ plugin shows connection error?

**A**: Check the plugin configuration：

1. Settings → Tools → Tabby → Server URL should be `http://localhost:8080`
2. If behind proxy, ensure `http_proxy` / `https_proxy` env vars are set for IntelliJ process
3. Check IDEA logs: Help → Diagnostic Tools → Debug Log → search "tabby"

### Q: Vim plugin not triggering suggestions?

**A**: Check autocmd registration：

```vim
" Verify the Tabby plugin is loaded
:filetype plugin indent on
:echo &omnifunc    " Should show tabby#complete or similar
```

---

## 14.5 SSO/OAuth Troubleshooting

### Q: OAuth callback URL mismatch?

**A**: The callback URL must be exactly：

```
https://your-tabby-domain.com/oauth/{provider}/callback
```

Where `provider` is one of: `github`, `gitlab`, `google`, `oidc`.

In your OAuth provider dashboard, set the redirect/callback URL to the above.

### Q: LDAP bind fails?

**A**: Verify connectivity and credentials：

```bash
# Test LDAP connection from server
ldapsearch -H ldap://ldap.yourcompany.com:389 \
  -D "cn=admin,dc=yourcompany,dc=com" \
  -w "password" \
  -b "ou=users,dc=yourcompany,dc=com" \
  "(uid=testuser)"

# Check firewall rules
telnet ldap.yourcompany.com 389
```

### Q: OIDC discovery failed?

**A**: Verify the OIDC issuer URL：

```bash
# Test OpenID discovery document is accessible
curl https://auth.example.com/.well-known/openid-configuration | jq .issuer
```

---

## 14.6 Database Issues

### Q: SQLite database corruption?

**A**: Restore from latest backup：

```bash
# Stop server first
docker compose down tabby

# Replace with backup（prod 编译的库文件是 ~/.tabby/ee/db.sqlite，dev 编译是 dev-db.sqlite）
cp /backup/tabby_latest.db ~/.tabby/ee/db.sqlite

# Start server
docker compose up -d tabby

# Verify migrations are current（sqlx 的记账表叫 `_sqlx_migrations`，不是 `_schema_migrations`）
sqlite3 ~/.tabby/ee/db.sqlite "SELECT version, description, installed_on FROM _sqlx_migrations ORDER BY version DESC LIMIT 1;"
```

### Q: Database file is too large?

**A**: Run VACUUM to reclaim space：

```sql
sqlite3 ~/.tabby/ee/db.sqlite "VACUUM;"
```

有一个**每小时**跑的自动清理作业 `DbMaintenanceJob`（`ee/tabby-webserver/src/service/background_job/db.rs · DbMaintenanceJob`，由 `HourlyJob::run` 调用），它删的是**行**：过期 refresh token、过期 password reset、过期 ephemeral thread、已失效 source 的 read access policy，外加对 `job_runs` / `user_events` 做 3 个月数据保留。

但它**不做 VACUUM**，所以删完行主库文件不会变小 —— 磁盘占用只能靠手工 `VACUUM` 回收（建议先停服：SQLite 在 WAL 模式下 VACUUM 需要独占写权限，运行中执行容易报 `database is locked`）。另一个 hourly 作业 `IndexGarbageCollection` 清的是 Tantivy 索引目录（`~/.tabby/index/`），跟 SQLite 无关。

---

## 14.7 Performance Tuning

### Q: Server is slow under high concurrency?

**A**: Check these bottlenecks：

| Bottleneck | Check | Fix |
|-----------|-------|-----|
| CPU threads | `top` / htop for tabby process | Increase `parallelism` config value |
| RAM pressure | `free -m` | Add more RAM or reduce context_size |
| Disk I/O | `iostat -x 1` | Move TABBY_ROOT to SSD/NVMe |
| GPU memory | `nvidia-smi` | Reduce num_gpu_layers, use smaller model |
| Rate limiting | Check 429 errors in logs | Increase rate_limit.request_per_minute |

### Q: Indexing takes too long?

**A**: Optimize indexing frequency and scope：

```toml
# config.toml — Skip large repos or specific patterns
[repositories]
[[repositories]]
git_url = "https://github.com/important-repo.git"
refs = ["main"]              # Only index main branch (default: all)
```

### Q: High disk usage from indexes?

**A**: Run garbage collection and adjust code_search params：

```bash
# Clean up stale index entries
tabby index gc

# Reduce indexing granularity
[answer]
code_search_params.num_to_score = 50     # Smaller candidate pool (default: 100)
```

---

## 14.8 Email Delivery Problems

### Q: SMTP test email not sent?

**A**: Check these items：

| Item | Verify |
|------|--------|
| Network reachability | `nc -vz smtp.example.com 587` |
| Credentials correct | Test with Thunderbird/Mail.app first |
| 加密方式不匹配 | Admin → Email 里的 `encryption` 字段（GraphQL 枚举 `Encryption`，源码变体 `StartTls` / `SslTls` / `None`，见 `ee/tabby-schema/src/schema/email.rs`）：25/587 用 `StartTls`，465 用 `SslTls`。**没有** `tls_enabled` 布尔开关 |
| 认证方式不匹配 | 同页的 `auth_method`（枚举 `AuthMethod`：`None` / `Plain` / `Login`）要与服务商要求对齐 |
| Port firewall | Some providers block port 25/587; use 465 (SSL) instead |

### Q: Emails go to spam?

**A**: Configure DNS records for your domain：

```bash
# SPF record
tabby._domainkey.yourcompany.com. IN TXT "v=DKIM1; k=rsa; p=..."

# DKIM/DMARC (for the domain sending emails)
_dmarc.yourcompany.com. IN TXT "v=DMARC1; p=quarantine;"
```

---

## 14.9 Quick Reference: Diagnostic Commands

### Server Health

```bash
# Full health check
curl http://localhost:8080/v1/health | jq

# Check if model is actually loaded
curl http://localhost:8080/v1/health | jq '.state'   # should be "ready"

# View current metrics
curl http://localhost:8080/metrics | grep tabby
```

### Database

```bash
# Check migration status（记账表 `_sqlx_migrations`，列见 ee/tabby-db/schema.sqlite）
sqlite3 ~/.tabby/ee/db.sqlite "SELECT version, description, success FROM _sqlx_migrations ORDER BY version;"

# Count entities（业务表都**不带**下划线前缀：users / repositories / threads / …）
sqlite3 ~/.tabby/ee/db.sqlite "SELECT 'users', count(*) FROM users UNION ALL SELECT 'repos', count(*) FROM repositories;"

# Disk usage
du -sh ~/.tabby/
```

### Docker

```bash
# View container logs
docker logs tabby --tail 100

# Check GPU in container
docker exec tabby nvidia-smi

# Inspect volume mount
docker inspect tabby --format='{{json .Mounts}}' | jq
```
