# life

Personal home OS web app. Rust (axum) backend + Angular frontend, own MariaDB.
Nextcloud is used only for **identity** (login) and **calendar** (CalDAV).

Design docs — [`overview.md`](docs/design/overview.md) (architecture and
boundaries), [`sync.md`](docs/design/sync.md) (local-first sync and its traps),
[`catalog-and-holdings.md`](docs/design/catalog-and-holdings.md) (products vs
items, shops, reconciliation), [`ui-grammar.md`](docs/design/ui-grammar.md).
[`docs/TODO.md`](docs/TODO.md) tracks what's built and what's next.

## Deploy

CI (`.github/workflows/build.yml`) gates on clippy + `cargo test`, then builds
and pushes the single image **`xinutec/life:latest`** to Docker Hub (backend
binary + built Angular bundle + scenes, all served from one process). The
Kubernetes manifests live in the **home monorepo** (`xinutec/pippijn` →
`code/kubes/life/k8s/`); the image tag is the only contract between this repo
and the running deployment.

There is **no image automation** (no Flux/Argo image controller): the Deployment
pins `xinutec/life:latest`, a fixed string, so pushing a new `:latest` changes
nothing on the cluster by itself. Once CI is green, roll it out from a monorepo
checkout — this is the single implementation, and it applies manifests and
restarts only what is behind:

```sh
~/Code/pippijn/code/kubes/deploy.sh life
```

It deploys the **host's** checkout, so it refuses unless the monorepo is on main,
committed and pushed. Then confirm the served bundle carries the new sha.

## Android app

A native-feeling phone wrapper — a full-screen WebView onto this site, no browser
chrome. Build & install steps: [`android/README.md`](android/README.md).

It also carries the *hidden* WebView the shop providers run in, which is why shop
work has historically needed the phone. It no longer does for the part that
changes: `scripts/shop-desktop.mjs` runs the same provider ops against a debug
Chrome on this machine.

```sh
node --experimental-strip-types scripts/shop-desktop.mjs waitrose search "black peppercorns"
node --experimental-strip-types scripts/shop-desktop.mjs waitrose product 785492
```

A `product` op needs a hand-made login to the shop in that Chrome profile: signed
out, Waitrose mints no Bearer at all, and the extractor says so. The session is
short-lived — expect to sign in again between sittings.

⚠ **Check the login on `waitrose.com/`, not on a search page.** The search page
the extractor runs on is identical signed in and signed out: both show "Sign in",
neither shows "Sign out". Probing it answers "signed out" for a signed-IN
session, which is why the extractor names the likely cause rather than claiming
to know it.

## Develop

```sh
nix develop          # Rust toolchain + sqlx-cli
./scripts/dev-db.sh  # local MariaDB on 127.0.0.1:3307 (data in .dev/, gitignored)
cp .env.example .env # then fill values; DATABASE_URL points at the dev DB
cargo run            # boots, migrates, serves on $BIND_ADDR
```

### Git hooks — one gate, at commit

A commit must be healthy: `gate.dhall` (compiled to `gate.json`) is the single
gate — backend fmt, clippy, the full test suite against a throwaway MariaDB,
generated-types drift; frontend lint, build, unit tests, ui-check; shared
dev-lint rules. It runs as a **pre-commit** hook — there is no separate pre-push
step. Slow by design; we optimise for healthy commits, not speed.

```sh
scripts/setup-hooks.sh   # activate, once per clone (sets core.hooksPath)
git commit --no-verify   # bypass for a genuine WIP commit
```

**A bare `cargo test` is not the test suite.** Most test files `return` early
unless `LIFE_TEST_DATABASE_URL` is set, so they report green with none of the SQL
exercised — and the queries are runtime strings, so *running* them is the only
check on them. The gate supplies a throwaway server; by hand:

```sh
LIFE_TEST_DATABASE_URL=mysql://life:life@127.0.0.1:3307/life cargo test
```

## Frontend

Angular 22 (Material 3) in `frontend/`. One origin: dev proxies to the backend,
prod is served by the backend.

```sh
cd frontend && pnpm install
pnpm start           # ng serve on :4200, proxies /api,/login,... to :8080
pnpm run build       # → frontend/dist/life-web/browser
```

Serve the built bundle from the backend by pointing `STATIC_DIR` at it:

```sh
STATIC_DIR=frontend/dist/life-web/browser cargo run
```


### Required environment

| Var               | Meaning                                              |
|-------------------|------------------------------------------------------|
| `DATABASE_URL`    | `mysql://life:<pw>@<host>/life`                      |
| `SESSION_SECRET`  | random string; HMAC key for session cookies          |
| `NC_BASE_URL`     | Nextcloud base URL, no trailing slash                |
| `NC_CLIENT_ID`    | OAuth2 client id (registered in NC admin)            |
| `NC_CLIENT_SECRET`| OAuth2 client secret                                 |
| `NC_REDIRECT_URI` | must match the OAuth2 client's redirect URI          |
| `BIND_ADDR`       | optional, default `0.0.0.0:8080`                     |

See `.env.example`. The two NC client values come from **Settings → Security →
OAuth 2.0** in Nextcloud admin; the redirect URI must be
`<app-origin>/auth/callback`.

## Routes (current)

- `GET  /login` → redirect to NC for sign-in (identity only)
- `GET  /auth/callback` → completes login, sets session cookie
- `POST /logout`
- `GET  /api/me` → `{ userId, displayName, nextcloud }`
- `POST /api/nextcloud/connect/init` → start CalDAV app-password link
- `GET  /api/nextcloud/connect/status` → `active | needs_reauth | not_linked`
- `GET  /healthz`

Migrations in `migrations/` run automatically at boot.
