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

# EE-specific integration tests
cargo test -p tabby --features "ee"  # Tests that require EE feature flag

# Benchmark tests
cargo bench -p tabby-inference       # Model inference performance benchmarks
```

### Test Categories

| Category | Location | Purpose |
|----------|---------|---------|
| **Unit** | Each crate's `#[cfg(test)] mod tests` | Individual function correctness |
| **Config parsing** | `tabby-common/src/config.rs` (tests module) | TOML config load/save/validation |
| **Model adapter** | `http-api-bindings/tests/`, `ollama-api-bindings/tests/` | Provider API compatibility |
| **Migration** | `tabby-db/src/lib.rs` integration tests | DB schema migration safety |

---

## 13.2 E2E / UI Tests

### Playwright E2E Tests (Admin Panel)

```bash
# Prerequisites: running server + browser binaries
pnpm exec playwright install chromium

# Run E2E tests against a live server
cd ee/tabby-ui
pnpm test:e2e                          # or: npx playwright test

# Debug mode (open browser UI)
npx playwright test --headed --debug
```

### Test Scenarios Covered

| Scenario | Steps | Verification |
|----------|-------|-------------|
| User registration | Register admin → login | Success redirect to dashboard |
| Model config change | Update model in Admin Panel | `/v1/health` reflects new model |
| GitHub integration | Add OAuth → sync repo | Repos appear in list |
| RBAC access control | Create user group + policy | Access denied for unauthorized users |
| Email delivery | Configure SMTP → send test email | `sent: true` in response |
| License update | Upload license → validate | Tier changes to Pro/Enterprise |

---

## 13.3 Python Evaluation Framework

### Location: `python/tabby-eval/`

#### Completion Eval Protocol

```python
# python/tabby-eval/tabby/model/__init__.py
class CompletionRequest:
    """Request to evaluate code completion quality."""
    prefix: str            # Code before cursor
    suffix: str            # Code after cursor (FIM)
    language: str          # Source file language
    vendor: str            # Model vendor name
    model: str             # Model identifier

class ChatCompletionRequest:
    """Request to evaluate chat/response quality."""
    messages: list[dict]   # [{role, content}, ...]
    model: str
```

#### Eval Workflow

```python
# 1. Load test dataset (typically from a JSONL file)
dataset = load_dataset("codegen_8M_seed0")  # Dataset of code snippets

# 2. For each example, call Tabby server
for example in dataset:
    response = tabby.post_completions({
        "language": example.language,
        "segments": {"prefix": example.prefix, "suffix": example.suffix},
    })

# 3. Compare generated code with ground truth
metrics = evaluate_completion(
    generated=response.text,
    expected=example.target,
    method="pass@k",      # pass@1, pass@10
)

# 4. Report results
print(f"pass@1: {metrics.pass_at_1:.4f}")
print(f"pass@10: {metrics.pass_at_10:.4f}")
```

### Eval Output Format

| Metric | Description | Good Range |
|--------|-------------|-----------|
| `pass@1` | First attempt matches ground truth | > 0.50 |
| `pass@10` | Any of top-10 attempts match | > 0.80 |
| `nll` | Negative log-likelihood (perplexity) | Lower is better |

---

## 13.4 Load Testing

### Location: `python/tabby-loadtest/`

#### Tools Used

| Tool | Purpose |
|------|---------|
| **k6 / locust** | HTTP load generation |
| **server.py** | Custom metrics collection |
| **loadtest.js** | Scripted test scenarios |

#### Load Test Scenario

```python
# python/tabby-loadtest/server.py — Metrics endpoint
from flask import Flask, jsonify
app = Flask(__name__)

metrics = {
    "total_requests": 0,
    "avg_latency_ms": 0,
    "p99_latency_ms": 0,
    "error_rate": 0,
}

@app.route("/metrics")
def get_metrics():
    return jsonify(metrics)

@app.route("/ingest", methods=["POST"])
def ingest_request():
    metrics["total_requests"] += 1
    latency = measure_time()
    metrics["avg_latency_ms"] = calculate_avg(latency)
    return jsonify({"status": "ok"})
```

#### Typical Load Test Configuration

| Parameter | Value | Description |
|-----------|-------|-------------|
| Concurrent users | 50 / 100 / 200 | Simultaneous IDE clients |
| Requests per user | 100/min | Completion requests per minute |
| Payload size | 2-5 KB avg | Average completion request body |
| Duration | 30 min | Test run length |

#### Expected Metrics (RTX 4090, Qwen2.5-Coder-7B)

| Metric | Value |
|--------|-------|
| p50 latency | ~50 ms |
| p99 latency | ~150 ms |
| Throughput | ~800 req/min (sustained) |
| GPU memory usage | ~6 GB (model + KV cache) |

---

## 13.5 Model Quality Evaluation

### Prompt Rewrite Eval (`experimental/prompt-rewrite-eval/`)

Evaluates how well the prompt builder constructs effective prompts from code context:

```bash
cd experimental/prompt-rewrite-eval
python eval.py \
  --model StarCoder-1B \
  --dataset codeparrot/github \
  --num-samples 1000 \
  --output results.json
```

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

#### Main CI (`ci.yml`)

```yaml
name: CI
on: [push, pull_request]

jobs:
  build-and-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          submodules: recursive

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: stable
          components: clippy, rustfmt

      - name: Install protobuf
        run: sudo apt install -y protobuf-compiler

      - name: Cargo fmt
        run: cargo fmt --all --check

      - name: Clippy
        run: cargo clippy --all-targets --all-features -- -D warnings

      - name: Build (open source)
        run: cargo build --release

      - name: Build (EE)
        run: cargo build --release --features ee

      - name: Test (core crates)
        run: cargo test -p tabby-common -p tabby-inference -p tabby-index -p tabby-git -p tabby-crawler

      - name: Test (tabby server)
        run: cargo test -p tabby

      - name: Test (EE DB)
        run: cd ee/tabby-db && cargo test --all-features

      - name: Upload coverage
        uses: codecov/codecov-action@v4
```

#### Coverage (`codecov.yml`)

```yaml
codecov:
  require_ci_to_pass: true

coverage:
  status:
    project:
      default:
        target: auto       # Auto-detect baseline
        threshold: 5%      # Allow 5% decrease
    patch:
      default:
        target: 80%        # New code must be >= 80% covered
```

---

## 13.7 Testing Model Backends Locally

### Mock Server for Dev

```python
# Create a mock server for testing new model adapters
pip install httpx-fastapi
# (see experimental/supervisor/ for existing test fixtures)
```

### Test Checklist for New Provider

| Step | Command | 通过标准 |
|------|---------|---------|
| Model download | `tabby download --model {name}` | GGUF files present |
| Health check | `curl -s /v1/health \| jq .state` | `ready` |
| Completion | `curl -X POST /v1/completions ...` | Returns candidate with text |
| Chat | `curl -X POST /v1/chat/completions ...` | Streams response |
| Embedding | `curl -X POST /v1/embeddings ...` | Returns float array |
| Multi-user | 50 concurrent requests | p99 < 500ms, 0 errors |
