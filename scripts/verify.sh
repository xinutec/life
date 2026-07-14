#!/usr/bin/env bash
# life verify — rust backend (fmt + clippy + tests) + angular frontend (build +
# unit tests) + shared rules.
#
# The backend's tests are integration tests against a real MariaDB, so they run
# only when LIFE_TEST_DATABASE_URL points at one (./scripts/dev-db.sh starts one);
# without it they are SKIPPED and this says so, loudly. It used to omit them
# silently, which let a rename of a column land green here and fail in CI — the
# queries are strings, so nothing else checks them. Keep them in the gate:
#
#   ./scripts/dev-db.sh &
#   LIFE_TEST_DATABASE_URL=mysql://life:life@127.0.0.1:3307/life ./scripts/verify.sh
set -euo pipefail
cd "$(dirname "$0")/.."
nix develop -c bash -c '
  set -euo pipefail
  # @angular/build:application tears down its Piscina worker pool at process
  # exit; on macOS / Node 24 / libuv 1.52 that teardown intermittently aborts
  # the process — a libuv kqueue assertion ("errno == EINTR", uv__io_poll →
  # Abort 6) or "EBADF: bad file descriptor, close" — AFTER "bundle generation
  # complete", i.e. once a complete, valid bundle is already on disk.
  # NG_BUILD_MAX_WORKERS=1 lowers the rate (fewer worker pipes to race) but does
  # NOT eliminate it; a spurious build abort here is worked around by re-running
  # verify. Harmless on Linux/CI, which build cleanly. NOT the sandbox.
  export NG_BUILD_MAX_WORKERS=1
  cargo fmt --all --check
  cargo clippy --all-targets -- -D warnings
  if [ -n "${LIFE_TEST_DATABASE_URL:-}" ]; then
    cargo test -- --test-threads=1
  else
    echo "verify: LIFE_TEST_DATABASE_URL unset — SKIPPING the backend tests."
    echo "verify: they are the only check on the SQL; CI runs them. To run them here:"
    echo "verify:   ./scripts/dev-db.sh &"
    echo "verify:   LIFE_TEST_DATABASE_URL=mysql://life:life@127.0.0.1:3307/life ./scripts/verify.sh"
  fi
  # Generated-types drift (formerly the separate pre-push gate): regenerate the
  # ts-rs bindings and fail if the committed frontend/src/app/generated output
  # moved. Needs cargo — this shell has it.
  scripts/check-types.sh
  # Frontend deps must exist before lint/build. verify.sh has to run from a clean
  # checkout (a fresh clone, or the tree the fleetwatch collector runs in) — not
  # just a warm dev machine — so install them when absent or the lockfile moved.
  if [ ! -d frontend/node_modules ] || [ frontend/package-lock.json -nt frontend/node_modules ]; then
    ( cd frontend && npm ci )
  fi
  # ui-check (L2 phone-width layout harness) runs after the build — it serves
  # the freshly-built dist and asserts no overlap/overflow at Pixel width.
  # See @xinutec/ui-harness + dev-lint/docs/layout-quality-architecture.md.
  ( cd frontend && npm run lint && npx ng build && npm test && npm run ui-check )
'
dev_lint_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/dev-lint"
[ -d "$dev_lint_dir" ] || dev_lint_dir="$HOME/Code/dev-lint"
[ -d "$dev_lint_dir" ] || dev_lint_dir="$HOME/code/dev-lint"
nix run "$dev_lint_dir" -- . # dev-lint
