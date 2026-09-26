#!/usr/bin/env bash
# Generate the frontend's wire types from the Rust types via ts-rs.
#
#   nix develop --command scripts/gen-types.sh            # regenerate + install
#   nix develop --command scripts/gen-types.sh --check    # report drift (the gate)
#
# The mechanics are dev-lint#gen-types; this holds only the paths and the cargo
# call. The export tests are named export_bindings_*, so the filter needs no
# database.
set -euo pipefail
cd "$(dirname "$0")/.."

# Pinned to dev-lint's committed HEAD: a path flake would build its working tree
# (see `withTestDb` in dev-lint/gate/schema.dhall).
exec nix run "git+file:../dev-lint?ref=HEAD#gen-types" -- "$@" \
  --out frontend/src/app/generated \
  -- cargo test export_bindings
