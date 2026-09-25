#!/usr/bin/env bash
# Generate the frontend TS interfaces from the Rust types via ts-rs, so the
# backend↔frontend wire shapes are consistent by construction, not transcribed.
#
#   nix develop --command scripts/gen-types.sh            # regenerate + install
#   nix develop --command scripts/gen-types.sh --check    # report drift, write nothing
#
# The second form is what the gate's generated-types row runs, so the cargo
# invocation below is stated once and both paths use it.
#
# The mechanics are dev-lint#gen-types, shared across the fleet; this file holds
# only where the bindings live and how to make cargo emit them.
#
# No `--features ts`: ts-rs is unconditional here. The export tests are named
# export_bindings_*, so the filter runs generation only and needs no database.
set -euo pipefail
cd "$(dirname "$0")/.."

# Pinned to dev-lint's committed HEAD: a path flake would build its working tree
# (see `withTestDb` in dev-lint/gate/schema.dhall).
exec nix run "git+file:../dev-lint?ref=HEAD#gen-types" -- "$@" \
  --out frontend/src/app/generated \
  -- cargo test export_bindings
