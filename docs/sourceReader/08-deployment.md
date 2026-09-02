# 第八章：部署指南

## 8.1 Deploy with Docker

### CPU-only Image

```bash
docker run -d \
  --name tabby \
  -p 8080:8080 \
  -v $HOME/.tabby:/data \
  tabbyml/tabby:latest \
  serve --model StarCoder-1B --device cpu

# Verify
curl http://localhost:8080/v1/health | jq '.state'   # → "ready"
```

### CUDA Image (NVIDIA GPU)

```bash
docker run -d \
  --name tabby \
  --gpus all \
  -p 8080:8080 \
  -v $HOME/.tabby:/data \
  tabbyml/tabby:cuda \
  serve --model Qwen2.5-Coder-7B --device cuda

# Verify GPU detection
curl http://localhost:8080/v1/health | jq '.arch'   # → "cuda"
```

### ROCm Image (AMD GPU)

```bash
docker run -d \
  --name tabby \
  --device /dev/kfd --device /dev/dri \
  --group-add video \
  -p 8080:8080 \
  -v $HOME/.tabby:/data \
  tabbyml/tabby:rocm \
  serve --model Qwen2.5-Coder-7B --device rocm
```

### Docker Image Tags

| Tag | GPU Backend | Base OS |
|-----|------------|---------|
| `latest` | CPU only | Ubuntu 22.04 |
| `cuda` | NVIDIA CUDA 11.7.1+ | Ubuntu 22.04 + CUDA runtime |
| `rocm` | AMD ROCm 5.x+ | Ubuntu 22.04 + ROCm runtime |

### docker-compose.yml (Production)

```yaml
version: "3.8"

services:
  tabby:
    image: tabbyml/tabby:cuda      # Change to :latest for CPU-only
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
# Rust toolchain (1.70+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Protobuf compiler
brew install protobuf          # macOS
apt install protobuf-compiler  # Ubuntu/Debian

# Node.js 20+ + pnpm 8+ (for ee/tabby-ui frontend)
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
  tabbyml/tabby:cuda \
  serve --model Qwen2.5-Coder-7B --device cuda
```

### HuggingFace Spaces

```python
# requirements.txt
tabby[cpu]==0.33.0.dev0

# app.py
from tabby import TabbyServer
server = TabbyServer(model="Qwen2.5-Coder-1.5B")
server.start()
```

### BentoML Deployment

```yaml
# bento.yml
name: tabby-server
compute:
  cuda: true
  gpu: 1
  cpu: 4
  memory: 16GB
  disk: 20GB
run:
  cmd: |
    docker run -d --gpus all -p 8080:8080 \
      -v ~/.tabby:/data tabbyml/tabby:cuda \
      serve --model Qwen2.5-Coder-7B --device cuda
```

### Modal

```python
import modal

app = modal.App(name="tabby")

image = (
    modal.Image.debian-slim()
    .apt_install("curl")
    .run_commands("pip install tabby[cpu]")
)

@app.cls(image=image, gpu=modal.gpu.A10G())
class TabbyServer:
    @modal.enter()
    def start(self):
        import subprocess
        subprocess.run(["tabby", "download", "--model", "Qwen2.5-Coder-7B"])
        subprocess.Popen(["tabby", "serve", "--device", "cuda"])
```

---

## 8.8 Production Hardening

### Security Checklist

| 项目 | 配置方式 |
|------|---------|
| HTTPS 强制 | Nginx/Caddy TLS 配置 + HTTP→HTTPS redirect |
| 身份认证 | EE OAuth/LDAP; Free: JWT token for all API |
| Rate Limiting | Admin Panel → Security Settings |
| CORS | restrict `server_setting.cors_allow_origins` |
| Secret Management | API keys in env vars (not config files) |
| DB Backup | Daily SQLite WAL backup (见 8.9) |
| Logging | Set `RUST_LOG=info` or `RUST_LOG=debug` |

### Environment Variables (Production)

```bash
# Server settings
export TABBY_HOST=0.0.0.0        # Listen on all interfaces
export TABBY_PORT=8080

# Logging
export RUST_LOG=info               # or debug for troubleshooting
export TABBY_LOG_FORMAT=json       # Structured JSON logs for log collectors

# Security
export TABBY_REGISTRATION_DISABLED=true   # Disable public registration
```

---

## 8.9 Database Backup & Restore

### SQLite Backup Procedures

```bash
# Method 1: COPY (safe, no downtime)
sqlite3 ~/.tabby/db/default.sqlite ".backup /backup/tabby-$(date +%Y%m%d).db"

# Method 2: pg_dump equivalent (SQL dump)
sqlite3 ~/.tabby/db/default.sqlite ".dump > /backup/tabby.sql"

# Method 3: WAL checkpoint before copy (ensure consistency)
sqlite3 ~/.tabby/db/default.sqlite "PRAGMA wal_checkpoint(TRUNCATE); COPY 'target.db'"
```

### Automated Backup Script

```bash
#!/bin/bash
# backup-tabby.sh
BACKUP_DIR="/backup/tabby"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
mkdir -p "$BACKUP_DIR"

sqlite3 ~/.tabby/db/default.sqlite "PRAGMA wal_checkpoint(TRUNCATE);"
sqlite3 ~/.tabby/db/default.sqlite ".backup '$BACKUP_DIR/tabby_$TIMESTAMP.db'"

# Keep last 30 days
find "$BACKUP_DIR" -name "tabby_*.db" -mtime +30 -delete

echo "Backup saved: $BACKUP_DIR/tabby_$TIMESTAMP.db"
```

### Restore from Backup

```bash
# Stop tabby first
docker compose down tabby

# Replace DB file
cp /backup/tabby_20250101_120000.db ~/.tabby/db/default.sqlite

# Start tabby again
docker compose up -d tabby
```

### Migration Safety

- All migrations use `sqlx-migrate-validate` at compile time
- Migrations are **forward-only** (no rollback scripts)
- Before migrating: always backup SQLite DB file
- Check migration history: `ee/tabby-db/migrations/` directory
