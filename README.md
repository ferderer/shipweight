# shipweight

Fast, cache-first package size analysis. A Rust-based replacement for [Bundlephobia](https://bundlephobia.com).

## Why

Bundlephobia serves ~300K API requests/day on a single server and is constantly rate-limited. The bottleneck is architectural: Node/Webpack-based bundling with no aggressive caching. Package sizes are immutable — once `react@19.0.0` is published, its bundle size never changes. A cache-first design turns this into a solved problem.

## Architecture

Two-process architecture with Postgres as message broker:

![Architecture](architecture.png)

- **Server** (Rust/Axum): Cache reads, job enqueuing, response serving. Stays up even if the worker crashes.
- **Worker** (TypeScript/esbuild): Claims jobs from Postgres, downloads tarballs, bundles with esbuild, compresses, writes results back.
- **Changes Feed** (Rust): Follows the npm registry changes feed, enqueues new versions of known packages into an idle queue for proactive caching.

Cache misses return **202 Accepted** with `Retry-After: 2`. Clients poll until the result is ready (200) or failed (422). After warmup, 95%+ of requests are served from the in-memory cache with zero I/O.

## What You Get Back

```
GET /v1/npm/react/19.0.0
```

```json
{
  "name": "react",
  "version": "19.0.0",
  "description": "React is a JavaScript library for building user interfaces.",
  "keywords": ["react"],
  "ecosystem": "npm",
  "size": 6420,
  "gzip": 2580,
  "brotli": 2190,
  "totalSize": 8300,
  "totalGzip": 3200,
  "totalBrotli": 2750,
  "dependencyCount": 1,
  "dependencyNames": ["loose-envify"],
  "treeshakeable": true,
  "sideEffects": false,
  "moduleFormat": "cjs",
  "repositoryUrl": "https://github.com/facebook/react",
  "homepage": "https://react.dev",
  "license": "MIT",
  "unpackedSize": 318938,
  "hasTypes": true,
  "monthlyDownloads": 130000000,
  "nodeEngine": ">=0.10.0",
  "maintainers": ["gnoff", "sebmarkbage"],
  "cachedAt": "2026-03-11T14:30:00Z"
}
```

Own sizes, total sizes (including transitive deps), treeshakeability, module format, types, license, downloads, maintainers — all extracted at zero extra cost.

## API

### Package Analysis

| Endpoint | Response | Description |
|---|---|---|
| `GET /v1/{eco}/{package}/{version}` | 200 / 202 / 422 | Analyze a specific version |
| `GET /v1/{eco}/@{scope}/{package}/{version}` | 200 / 202 / 422 | Scoped packages |
| `GET /v1/{eco}/{package}` | 200 / 404 | Latest cached version |
| `GET /v1/{eco}/@{scope}/{package}` | 200 / 404 | Latest cached (scoped) |

- **200**: Result ready — `{ "status": "ready", "result": { ... } }`
- **202**: Processing — `{ "status": "processing", "retryAfter": 2 }` (poll again)
- **422**: Failed — `{ "status": "failed", "error": "no entry point found" }`

### Discovery

| Endpoint | Description |
|---|---|
| `GET /v1/{eco}/search?q=router&keyword=react&sort=gzip&limit=20` | Fuzzy search with filters |
| `GET /v1/{eco}/top?sort=downloads&limit=20` | Top packages leaderboard |
| `GET /v1/{eco}/{package}/alternatives?limit=10` | Similar packages by keyword overlap |

**Search parameters:** `q` (fuzzy name), `keyword` (exact), `treeshakeable` (bool), `has_types` (bool), `sort` (gzip / total_gzip / size / total_size / name), `order` (asc / desc), `limit` (max 100), `offset`

### Compatibility

| Endpoint | Description |
|---|---|
| `GET /api/size?package=react@19.0.0` | Bundlephobia-compatible (sync, polls up to 30s) |

Returns `{ "name", "version", "size", "gzip" }` — drop-in replacement for existing VS Code extensions and CI tools.

### Badges

```
GET /badge/{ecosystem}/{package}.svg
GET /badge/{ecosystem}/@{scope}/{package}.svg
```

**Query parameters:** `metric`, `style` (flat / flat-square), `color` (hex or name), `label` (custom text)

**Supported metrics:**

| Metric | Example |
|---|---|
| `gzip` (default) | ![gzip](https://img.shields.io/badge/gzip-2.5_kB-brightgreen) |
| `size` / `minified` | Minified bundle size |
| `brotli` | Brotli-compressed size |
| `treeshakeable` / `tree-shaking` | ✓ or ✗ |
| `side-effects` / `sideEffects` | none or yes |
| `module` / `moduleFormat` | esm / cjs / dual |
| `dependencies` / `deps` | Dependency count |
| `types` | included or missing |
| `license` | MIT, Apache-2.0, etc. |
| `downloads` | Monthly npm downloads |
| `version` | Latest cached version |

Colors auto-scale: green (≤5 kB) → yellow (≤25 kB) → orange (≤100 kB) → red (>100 kB).

```markdown
![gzip size](https://shipweight.dev/badge/npm/react.svg)
![treeshakeable](https://shipweight.dev/badge/npm/react.svg?metric=treeshakeable)
![module](https://shipweight.dev/badge/npm/react.svg?metric=module&style=flat-square)
```

### Health

```
GET /health → { "status": "ok", "version": "0.1.0", "cache_entries": 12345 }
```

## Caching

**Two-layer cache with thundering-herd protection:**

- **L1 — moka** (in-memory LRU, 100K entries, ~50 MB): Handles 95%+ of requests. Built-in deduplication via `get_with()`.
- **L2 — PostgreSQL** (persistent): Immutable entries — npm doesn't allow republishing the same version. Denormalized for fast queries with `pg_trgm` GIN index on names and GIN index on keywords.
- **Negative cache** (L1-only, 1h TTL): Caches failures to prevent re-bundling broken packages.

**Request metrics:** Atomic `DashMap` counters per package, flushed to Postgres every 5 minutes. Used to prioritize cache warming.

## Spam Protection

Multi-layer defense against SEO spam packages:

- **Name validation:** npm naming rules + spam keyword detection (discount, coupon, casino, keygen, etc.) + SEO slug heuristic (≥5 hyphens and >40 chars)
- **API gate:** Invalid names rejected with 400 before enqueue
- **Changes feed filter:** Shared validation + maintainer blacklist loaded from `blocked_maintainers` table
- **Seed filter:** Minimum 100 monthly downloads + name validation

## Worker Pipeline

For each job:

1. Download tarball from npm registry (max 50 MB)
2. Find entry point (`exports` → `module` → `browser` → `main` → `index.js`)
3. Bundle with esbuild (`--bundle --minify --format=esm`, dependencies as `--external`)
4. Compress: gzip + brotli (quality 6)
5. Extract metadata: treeshakeable, module format, types, license, repository, homepage
6. Write to `size_cache`

**Resilience:** 3 retries (auto-adds unresolved imports as externals), 60s timeout per bundle, static blacklist (`@types/*`, `@babel/runtime`, `core-js`), permanent failure tracking.

**Two queues (priority):**
1. **Hot queue** (`job_queue`): User-requested packages — processed first
2. **Idle queue** (`idle_queue`): New versions of known packages — proactive cache warming via changes feed

## Database Schema

| Table | Purpose |
|---|---|
| `size_cache` | Denormalized package analysis results (PK: ecosystem, name, version) |
| `request_stats` | Per-package request counts for cache warming priority |
| `job_queue` | User-enqueued analysis jobs (pending → processing → done/failed) |
| `idle_queue` | Proactive caching jobs from changes feed |
| `failed_packages` | Permanent failures (unbuildable packages) |
| `blocked_maintainers` | Spam author blacklist |
| `metadata` | KV store (e.g., npm changes feed sequence number) |

## Multi-Ecosystem Design

Routes and queries are ecosystem-generic (`/v1/{ecosystem}/...`). npm is the first implementation. Adding an ecosystem means implementing the worker pipeline for that registry.

| Ecosystem | Metric | Bundling |
|---|---|---|
| npm | Bundle size (esbuild) | Yes |
| Maven | JAR size + transitive deps | No |
| Cargo | Crate download size + dep count | No |
| Composer | Package install size | No |
| PyPI | Wheel size per platform | No |

## Running

### Docker Compose (recommended)

```bash
# Development (includes local Postgres)
docker compose up -d

# Production (uses external Postgres, Traefik, VictoriaMetrics)
docker compose -f compose.yaml -f compose.prod.yaml up -d
```

**Services:** `shipweight` (API, 256 MB), `shipweight-worker` (bundler, 2 GB), `shipweight-changes-feed` (npm feed, 256 MB), `postgres` (dev only)

### Manual

```bash
# Build
cd server && cargo build --release

# Database
psql -d shipweight -f seed/init.sql

# Configure
export DATABASE_URL=postgresql://user:pass@localhost/shipweight
export RUST_LOG=shipweight=info
export PORT=3000

# API server
./target/release/shipweight

# Changes feed (separate process)
./target/release/shipweight --changes-feed

# Seed popular packages
./target/release/shipweight --seed

# Worker
cd worker && npm start
```

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | — | Postgres connection string |
| `PORT` | `3000` | API server port |
| `RUST_LOG` | — | Log level (`shipweight=info`) |
| `STATIC_DIR` | `./static` | SPA static file directory |
| `POLL_INTERVAL_MS` | `1000` | Worker poll interval |
| `MAX_DEPTH` | `10` | Max dependency recursion depth |
| `BROTLI_QUALITY` | `6` | Brotli compression level (must match across services) |

## Tech Stack

- **Server:** Rust, axum, tokio, sqlx, moka, reqwest, tower-http, serde, tracing
- **Worker:** TypeScript, esbuild, Node.js
- **Database:** PostgreSQL (pg_trgm, GIN indexes)
- **Infra:** Docker, Traefik, Cloudflare, VictoriaMetrics

## License

MIT
