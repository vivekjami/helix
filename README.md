# Helix

SIMD-accelerated near-duplicate detection and content freshness engine for web-scale crawl pipelines. Written in Rust with Node.js and Python bindings.

---

## What it does

Helix solves two problems that every large-scale crawler eventually hits:

**Near-duplicate detection** — finding pages that are semantically identical or near-identical without scanning the full index on every insert. Helix fingerprints each document using SimHash with SIMD-accelerated k-shingle hashing, then uses LSH band indexing to find candidates in sub-linear time.

**Content freshness tracking** — knowing whether a re-crawled URL actually changed or is just a re-fetch of identical content. Helix maintains a SHA-256 content hash and timestamp per URL, classifying each recrawl as `Unchanged`, `NearDuplicate`, or `New`.

Both operations work incrementally. No full index rebuilds.

---

## Architecture

```
helix-core/        # Rust library — fingerprinting, LSH index, freshness store
helix-api/         # Axum REST microservice wrapping helix-core
helix-node/        # NAPI-RS bindings (npm package)
helix-py/          # PyO3 + Maturin bindings (PyPI package)
benches/           # Criterion benchmarks
```

---

## Performance

| Operation | Throughput / Latency |
|---|---|
| SimHash fingerprinting | ~500k docs/sec (single core, AVX2) |
| LSH near-dup lookup | < 2ms p99 at 10M doc index |
| Freshness check (Redis) | < 1ms p99 |
| Incremental insert | O(b) band updates, no full rebuild |

Benchmarked on M2 MacBook Pro. AVX2 path used on x86_64 Linux in production.

---

## API (REST)

```
POST /fingerprint        → { url, fingerprint: u64, shingle_count: usize }
POST /dedup-check        → { url, fingerprint, status: "new"|"near_dup"|"exact_dup", candidates: [...] }
POST /freshness-check    → { url, content_hash, status: "unchanged"|"changed"|"new", last_seen_at }
POST /batch              → bulk version of the above, returns NDJSON
GET  /health             → { status: "ok", index_size, uptime_secs }
```

---

## Quickstart

```bash
# Clone and build
git clone https://github.com/vivekjami/helix
cd helix
cargo build --release

# Run the service
REDIS_URL=redis://localhost:6379 ./target/release/helix-api

# Node.js
npm install helix-node
```

```typescript
import { HelixClient } from 'helix-node';
const h = new HelixClient({ redisUrl: 'redis://localhost:6379' });
const result = await h.dedupCheck({ url, content });
// result.status → 'new' | 'near_dup' | 'exact_dup'
```

```python
import helix_py
idx = helix_py.HelixIndex(redis_url="redis://localhost:6379")
result = idx.dedup_check(url=url, content=content)
```

---

## Linting and CI

```bash
cargo fmt --check          # formatting
cargo clippy -- -D warnings  # lints, zero warnings policy
cargo test                 # unit + integration tests
cargo bench                # criterion benchmarks
```

GitHub Actions runs all four on every push and PR. Benchmarks are compared against `main` and fail the PR if p99 latency regresses more than 10%.

---

## Configuration

| Env var | Default | Description |
|---|---|---|
| `REDIS_URL` | `redis://localhost:6379` | Freshness store |
| `LSH_BANDS` | `24` | LSH band count (higher = more recall) |
| `LSH_ROWS` | `4` | Rows per band (higher = more precision) |
| `SIMHASH_BITS` | `64` | Fingerprint width |
| `HAMMING_THRESHOLD` | `3` | Max differing bits to call near-dup |
| `PORT` | `8080` | REST service port |

---

## Status

Active development. Core fingerprinting and LSH index are stable. Bindings and REST API are beta.
