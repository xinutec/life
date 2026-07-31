#!/usr/bin/env bash
# Generate the frontend TS interfaces from the Rust API types via ts-rs, so the
# backend↔frontend wire shapes are consistent by construction (not transcribed).
#
# Run inside the life dev shell (cargo on PATH):
#   nix develop --command scripts/gen-types.sh
#
# Output lands in frontend/src/app/generated/ (committed; imported via
# frontend/src/app/models.ts). The drift gate re-runs this and fails if the
# committed output no longer matches the Rust types — see scripts/check-types.sh.
#
# GENERATE-THEN-SWAP, never the other way round. This used to `rm -rf` the output
# first and run cargo with `>/dev/null 2>&1`; a crate that didn't compile then
# deleted 40-odd committed type files and said NOTHING. The tree looked like the
# types had simply vanished, and the real error — a Rust type error two modules
# away — was thrown away with them. So: build into a scratch dir, print cargo's
# output when it fails, and only replace the committed directory once there is
# something to replace it with. A failed run leaves the tree exactly as it was.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="frontend/src/app/generated"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/life-ts-rs.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

# ts-rs emits one file per #[ts(export)] type; the export tests are named
# export_bindings_*, so this filter runs only generation (no DB needed). The
# committed output dir is pinned in .cargo/config.toml (TS_RS_EXPORT_DIR); the
# env var overrides it so this run writes to the scratch dir instead.
if ! log=$(TS_RS_EXPORT_DIR="$TMP" cargo test export_bindings 2>&1); then
  printf '%s\n' "$log" >&2
  echo "gen-types: generation FAILED — $OUT left untouched" >&2
  exit 1
fi

count=$(find "$TMP" -name '*.ts' | wc -l | tr -d ' ')
if [ "$count" -eq 0 ]; then
  # cargo succeeded but emitted nothing — the export tests were filtered out or
  # renamed. Wiping a good directory over that is the same silent loss.
  echo "gen-types: cargo succeeded but emitted no types — $OUT left untouched" >&2
  exit 1
fi

# Copy the generated TYPES, not the scratch dir's contents. Anything else a build
# step decides to drop in $TMP would be copied into the committed output and then
# reported by the drift gate as unexplained drift — a confusing failure whose
# cause is invisible in the diff. Naming known strays one at a time only works for
# the ones already met.
#
# The whole output is replaced rather than merged, so a type deleted on the Rust
# side does not linger as a committed file nothing generates any more.
rm -rf "$OUT"
mkdir -p "$OUT"
find "$TMP" -maxdepth 1 -name '*.ts' -exec cp {} "$OUT"/ \;
echo "generated $count type(s) -> $OUT"
