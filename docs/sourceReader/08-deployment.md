# 第八章：部署指南

## 8.1 Deploy with Docker

### CPU-only Image

官方文档实测做法：同一个镜像换 entrypoint 到 CPU 二进制（见 `website/docs/quick-start/installation/docker.mdx`；没有单独的 CPU 镜像 tag）：

```bash
docker run -d \
  --name tabby \
  --entrypoint /opt/tabby/bin/tabby-cpu \
  -p 8080:8080 \
  -v $HOME/.tabby:/data \
  tabbyml/tabby \
  serve --model StarCoder-1B --chat-model Qwen2-1.5B-Instruct

# Verify
curl http://localhost:8080/v1/health | jq '.state'   # → "ready"
```

### CUDA Image (NVIDIA GPU)

docker.yml 只发布 CUDA 构建，默认 ENTRYPOINT 即 CUDA 版 tabby（`:cuda` 这个 tag 不存在）：

```bash
docker run -d \
  --name tabby \
  --gpus all \
  -p 8080:8080 \
  -v $HOME/.tabby:/data \
  tabbyml/tabby \
  serve --model Qwen2.5-Coder-7B --device cuda

# Verify GPU detection
curl http://localhost:8080/v1/health | jq '.arch'   # → "cuda"
```

### ROCm Image (AMD GPU)

仓库只有 `docker/Dockerfile.rocm`，没有发布 `:rocm` tag——以下为基于它自建镜像后的运行示例（示例镜像名）：

```bash
docker build -f docker/Dockerfile.rocm -t tabby-rocm-local .   # 自建

docker run -d \
  --name tabby \
  --device /dev/kfd --device /dev/dri \
  --group-add video \
  -p 8080:8080 \
  -v $HOME/.tabby:/data \
  tabby-rocm-local \
  serve --model Qwen2.5-Coder-7B --device rocm
```

### Docker Image Tags

实测 `.github/workflows/docker.yml`：只用 `docker/Dockerfile.cuda` 构建（matrix device-type = [cuda]），推送 Docker Hub `tabbyml/tabby` 与 `ghcr.io/tabbyml/tabby`，tag 由 metadata-action 生成——schedule 触发 `nightly` 与 `YYYYMMDD`，`v*` tag 触发 semver 版本号（并含 `latest`）；release 时另发 `-cuda11` 后缀（CUDA 11.7.1 兼容）。没有 `:cuda` / `:rocm` 这类 tag；`tabbyml/tabby:cuda`、`tabbyml/tabby:rocm` 是旧文档臆造：

| 变体 | 实际做法 |
|------|---------|
| CUDA（默认） | 镜像默认 ENTRYPOINT 即 `/opt/tabby/bin/tabby`（CUDA 构建） |
| CPU | 同一镜像，`--entrypoint /opt/tabby/bin/tabby-cpu`（见 `website/docs/quick-start/installation/docker.mdx`） |
| ROCm | 仓库提供 `docker/Dockerfile.rocm`，未接入 docker.yml 自动发布，需自建 |

官方指南统一用 `registry.tabbyml.com/tabbyml/tabby`（`website/docs/quick-start/installation/docker.mdx`、`docker-compose.mdx`）。

### docker-compose.yml (Production)

> 下面是一份**示例编排**（仓库中没有一个叫 `docker-compose.yml` 的根文件）。仓库内的真实参考栈是 `experimental/deploy/docker-compose.yaml`（tabby + scheduler + klaus + Caddy，配套 `experimental/deploy/Caddyfile`）与官方文档 `website/docs/quick-start/installation/docker-compose.mdx`。

```yaml
version: "3.8"

services:
  tabby:
    image: tabbyml/tabby           # 默认即 CUDA 构建；CPU 见 8.1 换 entrypoint（没有单独的 CPU 镜像 tag）
    container_name: tabby
    command: >
      serve --model Qwen2.5-Coder-7B
      --device cuda
      --chat-model Qwen2.5-Coder-1.5B-Instruct
    volumes:
      - tabby-data:/data           # Persistent data (DB, models, repos)
      - ./config.toml:/root/.tabby/config.toml:ro  # Read-only config
    ports:
      - "127.0.0.1:8080:8080"     # Bind to localhost for security
    environment:
      - TABBY_ROOT=/data
    deploy:
      resources:
        reservations:
          devices:
            - driver: nvidia
              count: 1
              capabilities: [gpu]
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/v1/health"]
      interval: 30s
      timeout: 10s
      retries: 3

volumes:
  tabby-data:
    driver: local
```

```bash
# Start
docker compose up -d

# Check logs
docker compose logs -f tabby

# Stop
docker compose down
```

### Volume Mounts

| Mount Path | 用途 | 是否需要持久化 |
|-----------|------|-------------|
| `/data` (mapped to `$HOME/.tabby`) | Models, SQLite DB, Git repos | **必须** |
| `config.toml` | Server configuration | 推荐只读挂载 |

---

## 8.2 Deploy from Source

### Prerequisites

```bash
# Rust toolchain（CI 固定 1.82.0，见 .github/workflows/test-rust.yml 与 docker.yml 的 RUST_TOOLCHAIN）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# CMake（crates/llama-cpp-server/build.rs 用 cmake 编译内置 llama.cpp；实测无需 protobuf-compiler）
apt install cmake

# Node.js + pnpm（for ee/tabby-ui frontend；CI 实测用 Node 18 + pnpm 9，见 .github/workflows/test-pnpm.yml）
curl -fsSL https://deb.nodesource.com/setup_20.x | bash -
apt install nodejs
npm install -g pnpm
```

### Build Commands

```bash
git clone --recurse-submodules https://github.com/TabbyML/tabby
cd tabby

# Option A: Open-source core only (no EE)
cargo build --release

# Option B: With Enterprise Edition features
cargo build --release --features ee

# GPU-specific builds
cargo build --release --features cuda    # NVIDIA CUDA
cargo build --release --features rocm    # AMD ROCm
cargo build --release --features vulkan  # Vulkan (cross-platform)
```

### Run from Source

```bash
# Download model weights first
./target/release/tabby download --model Qwen2.5-Coder-7B

# Start server
./target/release/tabby serve --model Qwen2.5-Coder-7B --device cuda
```

---

## 8.3 GPU Setup — NVIDIA CUDA

### Host Requirements

| 组件 | 最低版本 | 说明 |
|------|---------|------|
| NVIDIA Driver | 525+ | `nvidia-smi` shows CUDA 12.x |
| Docker | 20.10+ | With nvidia-container-toolkit |
| nvidia-container-toolkit | Latest | GPU passthrough for containers |

### Installation

```bash
# Install NVIDIA container toolkit (Ubuntu)
distribution=$(. /etc/os-release;echo $ID$VERSION_ID)
curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | sudo gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg
curl -sL https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list | sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' | sudo tee /etc/apt/sources.list.d/nvidia-container-toolkit.list
sudo apt-get update
sudo apt-get install -y nvidia-container-toolkit

# Configure Docker daemon
sudo nvidia-ctk runtime configure --runtime=docker
sudo systemctl restart docker

# Verify
docker run --rm --gpus all nvidia/cuda:12.2-base nvidia-smi
```

### Container GPU Passthrough

```bash
# In docker run command (already covered above)
--gpus all                          # Grant all GPUs
# Or specific GPU:
--gpus '"device=0,1"'              # Grant specific GPUs
```

---

## 8.4 GPU Setup — AMD ROCm

### Host Requirements

| 组件 | 最低版本 |
|------|---------|
| AMD Driver | 6.3+ (kfd module) |
| ROCm | 5.x or 6.x |
| Docker | 20.10+ |

### Installation

```bash
# Verify GPU detection
ls /dev/kfd /dev/dri
ls -la /dev/dri/by-path/

# In docker run:
--device /dev/kfd --device /dev/dri \
--group-add video
```

---

## 8.5 CPU-only Deployment

### RAM Requirements

| 模型大小 | 最小 RAM | 推荐 RAM |
|---------|---------|---------|
| 1B (Q4) | 6 GB | 8 GB |
| 3B (Q4) | 8 GB | 12 GB |
| 7B (Q4) | 10 GB | 16 GB |
| 7B (FP16) | 16 GB | 32 GB |
| 13B+ | 24 GB+ | 64 GB+ |

### Performance Expectations (CPU)

| 模型 | tokens/s (M1 MacBook) | tokens/s (8-core Intel) |
|------|----------------------|------------------------|
| StarCoder-1B (Q4) | ~50 | ~25 |
| Qwen2.5-Coder-7B (Q4) | ~15 | ~8 |

---

## 8.6 Reverse Proxy Setup

### Nginx Configuration

```nginx
server {
    listen 443 ssl http2;
    server_name tabby.yourcompany.com;

    # SSL certificates (use Let's Encrypt)
    ssl_certificate /etc/letsencrypt/live/tabby.yourcompany.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/tabby.yourcompany.com/privkey.pem;

    # Security headers
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;

    client_max_body_size 100M;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # WebSocket support (for chat streaming)
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";

        # SSE / streaming support
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 300s;
    }
}

# HTTP → HTTPS redirect
server {
    listen 80;
    server_name tabby.yourcompany.com;
    return 301 https://$host$request_uri;
}
```

### Caddy Configuration

```caddyfile
tabby.yourcompany.com {
    reverse_proxy localhost:8080

    # Automatic HTTPS (TLS cert)
    tls admin@yourcompany.com

    header {
        X-Frame-Options "SAMEORIGIN"
        X-Content-Type-Options "nosniff"
    }

    encode gzip zstd
}
```

### Traefik Configuration (Docker Swarm/K8s)

```yaml
# docker-compose.yml for Traefik
services:
  tabby:
    labels:
      - "traefik.http.routers.tabby.rule=Host(`tabby.yourcompany.com`)"
      - "traefik.http.routers.tabby.tls=true"
      - "traefik.http.services.tabby.loadbalancer.server.port=8080"
```

---

## 8.7 Cloud Deployment Guides

### AWS (EC2 + CUDA)

> 仓库内的官方 GPU 云指南实测是 SkyPilot：`website/docs/references/cloud-deployment/skypilot/index.md` + `tabby.yaml`。下面的裸 EC2 命令是通用示例（仓库中不存在对应脚本），注意 CUDA 镜像没有 `:cuda` tag，默认 entrypoint 就是 CUDA 构建：

```bash
# Launch an instance with GPU
aws ec2 run-instances \
  --instance-type g5.xlarge \    # 1x A10G, 4 vCPU, 24GB RAM
  --ami ami-0xxxxxxx \           # Ubuntu 22.04 AMI
  --security-group-ids sg-xxxxxx \
  --key-name my-key

# On the instance
sudo apt update && sudo apt install -y docker.io curl git
sudo usermod -aG docker $USER
newgrp docker

docker run -d --gpus all -p 8080:8080 -v ~/.tabby:/data \
  tabbyml/tabby \
  serve --model Qwen2.5-Coder-7B --device cuda
```

### HuggingFace Spaces

Tabby **没有发布 Python 包**（不存在 `pip install tabby`，也没有 `from tabby import TabbyServer` 这样的 API），HF Spaces 走的是 **Docker Space**：复制官方模板 space `TabbyML/tabby-template-space` 即可，详见 `website/docs/references/cloud-deployment/hugging-face/index.md`。

要点（照该文档写死的事实）：
- Space 起来后得到形如 `https://UserName-SpaceName.hf.space` 的 Direct URL，把它填进 IDE 插件的 endpoint。
- Private Space 需要鉴权：编辑 **agent 侧**配置 `~/.tabby-client/agent/config.toml`，解开 `[server]` 与 `[server.requestHeaders]` 两段，用 HF Access Token 拼 Authorization 头。
- 官方文档本身也镜像在 huggingface 的 `spaces-sdks-docker-tabby` 页面。

### BentoML Deployment

仓库里有可直接照抄的样例，位于 `website/docs/references/cloud-deployment/bentoml/`：`bentodeploy.yaml`、`service.py`、`setup-docker.sh`、`index.md`（**没有 `bento.yml` 这个文件**）。

真实形态与常见想象的差别：BentoML 侧不是「一条 `docker run`」，而是用 Python service 以子进程方式拉起 tabby，再用 ASGI proxy 转发。
```python
# website/docs/references/cloud-deployment/bentoml/service.py（节选）
class TabbyServer:
    def __init__(self, model_id: str, chat_model_id: str) -> None:
        self.launcher = subprocess.Popen(
            ["tabby", "serve", "--model", model_id, "--chat-model", chat_model_id,
             "--device", "cuda", "--port", "8000"]
        )
...
app = asgi_proxy("http://127.0.0.1:8000")

@bentoml.service(resources={"gpu": 1, "gpu_type": "nvidia-l4"}, traffic={"timeout": 10})
@bentoml.mount_asgi_app(app, path="/")
class Tabby:
    @bentoml.on_deployment
    def prepare(): download_tabby_dir("tabby-local")
    @bentoml.on_shutdown
    def shutdown(self): upload_tabby_dir("tabby-local")
```
`bentodeploy.yaml` 的 `name: tabby` + `bento: ./`，并用 envs 设 `TABBY_MODEL_CACHE_ROOT=/home/bentoml/tabby-models`（这正是 `crates/tabby-common/src/path.rs · models_dir()` 认的那个环境变量，用来把模型缓存放进可持久化的卷）与一组 `RCLONE_CONFIG_R2_*` 用于部署前后打包/还原 `~/.tabby`。

### Modal / Skypilot

- Modal 官方指南与脚本实测在 `website/docs/references/cloud-deployment/modal/`（`index.md` + `app.py`）。`app.py` 的真实形态：`IMAGE_NAME = "tabbyml/tabby"`，`MODEL_ID / CHAT_MODEL_ID / EMBEDDING_MODEL_ID / GPU_CONFIG(gpu.T4())` 常量；构建镜像时清空 ENTRYPOINT、用 `/opt/tabby/bin/tabby download` 预下载权重（省冷启动）；`@asgi_app` 函数内拉起 `tabby serve` 后 `asgi_proxy` 转发。仓库内另有两个可直接照抄的 Modal 脚本：`python/tabby-loadtest/server.py`（压测栈）与 `python/tabby-eval/modal/predict.py`（评估栈）。旧文档写的 `pip install tabby[cpu]` 不存在——PyPI 没有 tabby 包，只能基于 docker 镜像。
- SkyPilot 指南：`website/docs/references/cloud-deployment/skypilot/index.md` + 同目录 `tabby.yaml`。

---

## 8.8 Production Hardening

### Security Checklist

| 项目 | 配置方式 |
|------|---------|
| HTTPS 强制 | Nginx/Caddy TLS 配置 + HTTP→HTTPS redirect |
| 身份认证 | EE OAuth/LDAP; Free: JWT token for all API |
| Rate Limiting | Admin Panel → Security Settings |
| CORS | 实测 API 路由直接挂 `CorsLayer::permissive()`（`crates/tabby/src/routes/mod.rs`），仓库中没有 `cors_allow_origins` 这类配置项——要收紧只能在反向代理层做 |
| Secret Management | API keys in env vars (not config files) |
| DB Backup | Daily SQLite WAL backup (见 8.9) |
| Logging | logkit/tracing 输出到 stdout；实测代码不读 `RUST_LOG` 环境变量（不存在该开关） |

### Environment Variables (Production)

```bash
# 实测全仓 env::var 读取的 TABBY_* 变量（不是旧文档臆造的 TABBY_HOST/TABBY_PORT 等）：
export TABBY_ROOT=$HOME/.tabby             # crates/tabby-common/src/path.rs · TABBY_ROOT
export TABBY_MODEL_CACHE_ROOT=/data/models # 模型缓存重定向（同文件 models_dir()）
export TABBY_DISABLE_USAGE_COLLECTION=1    # crates/tabby-common/src/usage.rs 匿名用量上报开关
export TABBY_DOWNLOAD_HOST=...             # crates/tabby-download/src/lib.rs 模型下载域名
export TABBY_HUGGINGFACE_HOST_OVERRIDE=... # HF 域名覆盖（modelscope 镜像，同文件）
export TABBY_EMBEDDING_ENABLED=...         # crates/tabby-common/src/config.rs
export TABBY_CRAWL_ENABLE_HEADLESS=1       # crates/tabby-crawler/src/lib.rs

# 监听地址/端口不是环境变量，而是 serve 参数：
# tabby serve --host <0.0.0.0> --port <8080>（默认值见 crates/tabby/src/serve.rs · ServeArgs）
# 注册开关也不走 env：在 Admin Panel（EE）里控制；TABBY_REGISTRATION_DISABLED / TABBY_LOG_FORMAT 不存在
```

---

## 8.9 Database Backup & Restore

> DB 文件的真实位置由 `ee/tabby-webserver/src/path.rs · db_file()` 决定：`~/.tabby/ee/db.sqlite`（`prod` feature）或 `~/.tabby/ee/dev-db.sqlite`（默认 dev 构建），目录是 `tabby_ee_root()` = `tabby_root()/ee`。**不是** `~/.tabby/db/default.sqlite`，也不是 `webserver.db`。

### SQLite Backup Procedures

```bash
DB=~/.tabby/ee/db.sqlite   # dev 构建下是 ~/.tabby/ee/dev-db.sqlite

# Method 1: .backup (safe, no downtime; SQLite 自带的在线备份命令)
sqlite3 "$DB" ".backup /backup/tabby-$(date +%Y%m%d).db"

# Method 2: SQL dump (文本导出，便于人工核对 / 迁移到别的引擎)
sqlite3 "$DB" ".dump" > /backup/tabby.sql

# Method 3: 先做 WAL checkpoint 把 -wal 里的内容压回主库，再复制
sqlite3 "$DB" "PRAGMA wal_checkpoint(TRUNCATE);"
cp "$DB" /backup/tabby-$(date +%Y%m%d).db
```

> WAL 是在 `ee/tabby-db/src/lib.rs` 里用 `SqliteConnectOptions::journal_mode(Wal)` 打开的（文件头注释引用了两篇关于启用 WAL 的资料），所以上面第 3 种做法在进程仍在写时并**不**保证一致快照——要真正稳妥，用 Method 1 的 `.backup`。

### Automated Backup Script

> 仓库里**没有** `backup-tabby.sh`（根 `ci/` 只有 `package-from-upstream.sh` 与 `prepare_build_environment.{sh,ps1}`）。下面是一段自建脚本示例，放到自己的 cron 里即可。

```bash
#!/bin/bash
# 自建脚本示例（仓库中不存在此文件）：backup-tabby.sh
BACKUP_DIR="/backup/tabby"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
mkdir -p "$BACKUP_DIR"

DB="${TABBY_ROOT:-$HOME/.tabby}/ee/db.sqlite"
sqlite3 "$DB" ".backup '$BACKUP_DIR/tabby_$TIMESTAMP.db'"

# Keep last 30 days
find "$BACKUP_DIR" -name "tabby_*.db" -mtime +30 -delete

echo "Backup saved: $BACKUP_DIR/tabby_$TIMESTAMP.db"
```

### Restore from Backup

```bash
# Stop tabby first
docker compose down tabby

# Replace DB file
cp /backup/tabby_20250101_120000.db ~/.tabby/ee/db.sqlite

# Start tabby again
docker compose up -d tabby
```

### Migration Safety

- All migrations use `sqlx-migrate-validate` at compile time
- 实测并非「forward-only」：`ee/tabby-db/migrations/` 里每个迁移都是成对的 up/down 文件（如 `0001_registration-token-table.up.sql` 与 `0001_registration-token-table.down.sql`），可回滚，只是运行期默认向前应用
- Before migrating: always backup SQLite DB file
- Check migration history: `ee/tabby-db/migrations/` directory
