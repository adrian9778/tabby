# 第十三章：测试与评估

## 13.1 Unit & Integration Tests

### Rust Test Suite

```bash
# Run all workspace tests
cargo test --all-targets --all-features

# Per-crate testing
cargo test -p tabby                 # Server binary + routes + services
cargo test -p tabby-common           # Config parsing, type validation
cargo test -p tabby-inference        # Inference trait implementations
cargo test -p tabby-index            # Indexing pipeline
cargo test -p tabby-db               # SQLite layer (requires no DB running)
cargo test -p tabby-webserver        # Webserver service layer tests

# Golden tests（快照式端到端：completions / chat，见 crates/tabby/tests/）
cargo test -p tabby --test goldentests
cargo test -p tabby --test goldentests_chat

# Community build（去掉 EE；`ee` 本是 tabby 的 default feature，见 crates/tabby/Cargo.toml · [features]）
cargo test --bin tabby --no-default-features
```

> 仓库内没有 `[[bench]]` 基准目标；上面 `goldentests` / `goldentests_chat` 与 `cargo test --doc` 是 CI 实际跑的 Rust 测试面（`.github/workflows/test-rust.yml`）。

### Test Categories

| Category | Location | Purpose |
|----------|---------|---------|
| **Unit** | Each crate's `#[cfg(test)] mod tests` | Individual function correctness |
| **Config parsing** | `crates/tabby-common/src/config.rs` (tests module) | TOML config load/save/validation |
| **Golden tests** | `crates/tabby/tests/goldentests.rs`, `goldentests_chat.rs` + `snapshots/` | completions/chat 的快照式端到端回归 |
| **Migration** | `ee/tabby-db/src/migration_tests.rs`, `crates/sqlx-migrate-validate` | DB schema migration safety |

（`http-api-bindings`、`ollama-api-bindings` 两个 crate 下没有 `tests/` 目录——provider 兼容性主要靠上述 golden tests 覆盖。）

---

## 13.2 E2E / UI Tests

### Frontend Tests (Admin Panel)

实测：仓库内没有任何 Playwright 配置或 `test:e2e` 脚本（不存在），`ee/tabby-ui` 用的是 **vitest**：

```bash
# ee/tabby-ui/package.json · "test": "vitest run"
cd ee/tabby-ui
pnpm test

# CI 入口：.github/workflows/test-pnpm.yml（PR 触发，覆盖 clients/** 与 ee/tabby-ui/**）
# 依次执行 pnpm lint / pnpm test / build
```

### Test Scenarios Covered

测试代码实际分布（`ls` 实测）：

| Area | Location | Runner |
|------|----------|--------|
| Admin Panel 组件测试 | `ee/tabby-ui/test/`（index.test.ts、components/chat 等） | vitest |
| IDE agent 单测 | `clients/tabby-agent/src/**/*.test.ts`（config、completion、postprocess 等） | vitest（test-pnpm.yml） |
| IntelliJ 插件 | `.github/workflows/test-intellij.yml` | gradle test |
| 服务端 E2E | `crates/tabby/tests/goldentests*.rs`（见 13.1） | cargo test |

---

## 13.3 Python Evaluation Framework

### Location: `python/tabby-eval/`

实测形态：`python/tabby-eval/` 现在只剩 `modal/` 一个子目录，内含评估入口 `python/tabby-eval/modal/predict.py` 和自动生成的 API client `python/tabby-eval/modal/tabby_python_client/`。旧版文档描述的多文件评估框架（`tabby/` 包、独立的打分脚本等）在仓库中不存在。它的真实职责是：**把 Tabby server 部署到 Modal，对 JSONL 数据集批量跑补全预测并回写结果**，打分需离线基于回写列自行计算。

#### 请求协议

`CompletionRequest`（自动生成于 `python/tabby-eval/modal/tabby_python_client/tabby_python_client/models/completion_request.py · class CompletionRequest`）字段为 `language / segments / user / debug_options`；`segments` 即 `{prefix, suffix}`。预测走的是「`DebugOptions(raw_prompt=...)` 直接投喂完整 prompt」这条路，而不是让服务端拼 prompt。

#### Eval Workflow（按 `predict.py` 源码实测复述）

```text
1. 环境变量 MODEL_ID 选定模型；镜像基于 Image.from_registry("tabbyml/tabby:...")，
   构建期执行 /opt/tabby/bin/tabby download --model $MODEL_ID。
2. Model 类容器内拉起 `tabby serve --model $MODEL_ID --port 8000 --device cuda`，
   轮询 127.0.0.1:8000 直到 webserver 就绪。
3. 入口 main(language, files) 读取 ./data/{model_name}/{language}/{file} 的 JSONL；
   每行取 prompt（若有 crossfile_context.text 则拼在其前面）作为 raw_prompt。
4. 每 30 行一批，asyncio.gather 并发调用 v1/completion；
   成功行写回 `prediction` 列，失败行写 `error` 列（就地覆盖原文件）。
5. 运行日志追加到 ./modal/log.txt（含跳过行数、成功/失败计数）。
```

### Eval Output Format

评估「输出」就是回写后的 JSONL（`prediction` / `error` 列）。下表指标是离线打分时的常用定义（参考性质，`predict.py` 本身不产出这些数值，仓库中也没有内置这些打分的脚本）：

| Metric | Description |
|--------|-------------|
| `pass@k` | 取 K 次生成，任意一次通过测试即算通过 |
| `exact match` | 生成文本与 ground truth 的字符级一致率 |
| `nll` | 负对数似然（困惑度） |

---

## 13.4 Load Testing

### Location: `python/tabby-loadtest/`

#### Tools Used（实测目录只有 server.py / loadtest.js / run.sh / README.md / record.csv）

| Tool | Purpose |
|------|---------|
| **Modal** (`server.py`) | 部署被测服务端：`@asgi_app` 拉起 `tabby serve` 并用 asgi_proxy 转发（不是指标采集器，仓库里没有 locust） |
| **k6** (`loadtest.js`) | 压测脚本：对 `/v1/completions` 发固定 Python prefix 请求并校验阈值 |
| **run.sh** | 编排：`modal deploy server.py` → 轮询 `/v1/health` → `k6 run loadtest.js` → 指标追加进 `record.csv`；`dichotomic_search` 对并发上限做二分搜索 |

#### Load Test Scenario

```python
# python/tabby-loadtest/server.py（实测源码）
# - 环境变量：GPU_CONFIG（默认 T4）/ MODEL_ID（默认 TabbyML/StarCoder-1B）/ PARALLELISM（默认 4）
# - 镜像 tabbyml/tabby，清空 ENTRYPOINT，构建期执行 tabby download --model $MODEL_ID
# - 函数内启动 `tabby serve --port 8000 --device cuda --parallelism $PARALLELISM`，
#   轮询就绪后 return asgi_proxy("http://localhost:8000")
```

#### Typical Load Test Configuration（按 `loadtest.js` 实测）

| Parameter | Value | Description |
|-----------|-------|-------------|
| Stages | 1s + 30s ramp 到 `PARALLELISM` | 虚拟用户数由环境变量决定 |
| Threshold | `http_req_failed rate<0.001`；`med/avg<1800ms`、`p(90)<2500ms`、`p(95)<3000ms` | 注释标明是对 StarCoder-1B + T4 校准的 |
| Payload | 固定 Python `binarySearch` prefix，`sleep(0.5)` | 每 VU 半秒一次请求 |

#### 实测记录（`record.csv`，58 行历史结果，延迟单位为秒）

| GPU | Model | Parallelism | QPS | Avg Latency | Passed |
|-----|-------|-------------|-----|-------------|--------|
| T4 | TabbyML/StarCoder-1B | 7 | 4.14 | 1.69 | SUCCESS |
| T4 | TabbyML/StarCoder-1B | 10 | 4.85 | 2.06 | FAILED |
| A10G | TabbyML/StarCoder-1B | 17 | 12.01 | 1.42 | SUCCESS |
| A10G | TabbyML/StarCoder-1B | 25 | 14.61 | 1.71 | SUCCESS |

注：`record.csv` 覆盖 T4/A10G/A100 × StarCoder-1B/3B、DeepseekCoder-1.3B/6.7B、CodeLlama-7B/13B，仓库中没有 RTX 4090 的测量记录。

---

## 13.5 Model Quality Evaluation

### Prompt Rewrite Eval (`experimental/prompt-rewrite-eval/`)

目录实测只有 `README.md / evaluator.py / dashboard.py / eval.toml / requirements.txt`；旧文档写的 `python eval.py --model ... --output results.json` 式命令行入口在仓库中不存在（README 中提到的 `evaluation.py` 不存在，实际文件名是 `evaluator.py`）。

```bash
cd experimental/prompt-rewrite-eval
pip install -r requirements.txt
# 1. 先编辑 experimental/prompt-rewrite-eval/eval.toml：
#    tabby_path / index_repo_url / sample_repo_url / language / prompt_count
python ./evaluator.py
```

实际流程（按 `experimental/prompt-rewrite-eval/evaluator.py` 源码实测）：

1. 把 `index_repo_url` 写入 `~/.tabby/config.toml`（含 `experimental.enable_prompt_rewrite = true`），跑 `tabby scheduler --now` 建索引；
2. 用独立 `TABBY_ROOT`（`~/.tabby/eval_sample`）索引 `sample_repo_url`，读其 `dataset/*.jsonl`，随机挑光标位置、前后各 10 行生成 prefix/suffix 段；
3. 拉起 `tabby serve --model TabbyML/T5P-220M`，把各段 POST 到 `/v1/completions`；
4. prompt 事件由服务端落盘到 `~/.tabby/events/` 下的日切 jsonl；用 `streamlit run ./dashboard.py` 按语言浏览最近 prompt（结果不落单独文件，`results.json` 不存在）。

### Completion Quality Metrics

| Metric | Measurement | Formula |
|--------|------------|---------|
| **Acceptance Rate** | Events / Impressions | `select_events / view_events` |
| **Char-level Accuracy** | Generated vs. Ground Truth | `char_match / total_chars` |
| **Token-level BLEU** | N-gram precision | Standard BLEU on tokenized output |
| **Pass@K** | Code correctness via tests | Fraction where any of K attempts passes |

---

## 13.6 CI Pipeline Details

### GitHub Actions Workflows (`.github/workflows/`)

#### Rust 测试（`test-rust.yml`，实测源码）

触发：push 到 main / PR，且命中 `Cargo.toml`、`Cargo.lock`、`crates/**`、`ee/**`（排除 `ee/tabby-ui/**`）。工具链固定 `RUST_TOOLCHAIN: 1.82.0`，先跑 `ci/prepare_build_environment.sh`，编译用 sccache + cargo registry 缓存。

jobs：
- `tests`（仅 PR）：`cargo test --doc` → `cargo test --bin tabby --no-default-features`（community build）→ `cargo test --bin tabby --lib`
- `coverage`：`CI_COVERAGE=1 cargo llvm-cov --bin tabby --lib --lcov`，用 codecov-action@v3 上传 `lcov.info`

> 旧文档写的单一 `ci.yml`（含 fmt/clippy/protobuf/codecov target=80% 等步骤）在仓库中不存在；格式与 lint 由 `ast-grep-lint.yml` 和三个 `autofix-*.yml` 承担。

#### 真实 workflow 清单（ls 实测，逐个打开核对过 name 与触发条件）

| 文件 | 名称 | 职责 |
|------|------|------|
| `test-rust.yml` | Test Rust | Rust doc/单测/覆盖率 |
| `test-pnpm.yml` | Test Pnpm | clients/** + ee/tabby-ui：pnpm lint/test/build |
| `test-intellij.yml` | Test IntelliJ | IntelliJ 插件测试 |
| `docker.yml` | Create and publish docker image | 每日 schedule + `v*` tag 构建推送 `tabbyml/tabby`（cuda，`docker/Dockerfile.cuda`；v tag 另发 `-cuda11` 后缀） |
| `nightly.yml` | Update nightly tag | 每天滚动 `nightly` git tag |
| `release.yml` | Build and release binaries. | `v*`/`nightly` tag 发布二进制 |
| `release-vscode.yml` / `release-vim.yml` / `release-intellij.yml` | — | 三个客户端插件发布 |
| `autofix-rust.yml` / `autofix-pnpm.yml` / `autofix-python.yml` | autofix.ci | 自动修复格式/lint 依赖 |
| `ast-grep-lint.yml` | ast-grep lint | ast-grep 静态规则检查 |
| `bloat.yml` | bloat | PR 上的 Cargo 依赖膨胀检查 |
| `gpt-translate.yml` | GPT Translate | issue/PR 评论触发的多语言翻译 |
| `gh-pages.yml` | Deploy static content to Pages | `website/**` 变更后发布文档站 |

#### Coverage (`codecov.yml`)

仓库根 `codecov.yml` 实测内容（旧文档写的 `require_ci_to_pass`、patch target 80% 不存在）：

```yaml
coverage:
  status:
    project:
      default:
        threshold: 0%
    patch:
      default:
        target: 75%
        threshold: 0%
```

---

## 13.7 Testing Model Backends Locally

### Mock Server for Dev

```python
# Create a mock server for testing new model adapters
pip install httpx-fastapi
```

> 注：`experimental/supervisor/` 里只有一个 `app.py`——用 uvicorn + asgi_proxy 拉起并转发 `tabby serve` 的开发启动器（实测），并不是测试 fixture 库；mock 单测请看各 crate 内 `#[cfg(test)]`（如 `crates/tabby/src/services/completion.rs` 里的 `MockCodeSearch`）。

### Test Checklist for New Provider

| Step | Command | 通过标准 |
|------|---------|---------|
| Model download | `tabby download --model {name}` | GGUF files present |
| Health check | `curl -s /v1/health \| jq .state` | `ready` |
| Completion | `curl -X POST /v1/completions ...` | Returns candidate with text |
| Chat | `curl -X POST /v1/chat/completions ...` | Streams response |
| Embedding | `curl -X POST /v1/embeddings ...` | Returns float array |
| Multi-user | 50 concurrent requests | p99 < 500ms, 0 errors |
