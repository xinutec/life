#!/usr/bin/env bash
# Emit the SQL that re-blesses migrations whose COMMENTS changed.
#
# `sqlx::migrate!()` stores a SHA-384 of each migration file and compares it on
# every boot, so editing an already-applied migration — even one comment
# character — makes the app refuse to start against a database that ran it.
# That is the right default: it is what stops someone editing the SQL of a
# migration that has already shaped a real database.
#
# But it also freezes the prose, and migration headers in this repo carry the
# reasoning for the schema. This script is the supported way to edit that prose:
# it re-blesses a migration ONLY when the SQL is untouched, and refuses
# otherwise, so the guard sqlx provides is narrowed rather than bypassed.
#
# Usage:
#   scripts/rechecksum-migrations.sh [ref]      # ref defaults to HEAD
#
# It writes UPDATE statements to stdout and explanation to stderr. Apply them to
# EVERY database that already ran the migration — production and each dev
# database — in the same breath as deploying the matching image. A database and
# a binary that disagree will not boot.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

ref="${1:-HEAD}"

# Comment and blank lines removed, so "did the SQL change?" ignores prose. Only
# whole-line `--` comments are stripped: an inline trailing comment stays part of
# the compared text, so editing one reads as an SQL change and this refuses. That
# is the safe direction to be wrong in.
sql_only() { grep -vE '^[[:space:]]*(--.*)?$' || true; }

# Statements are buffered, never streamed. If any migration is refused, nothing
# reaches stdout at all — otherwise piping this straight into a database would
# apply the earlier statements before the refusal was reached, which is the
# failure this tool exists to prevent.
statements=""
changed=0
blocked=0

while IFS= read -r file; do
  [ -n "$file" ] || continue
  version="$(basename "$file" | sed 's/^0*//; s/_.*//')"

  if ! git cat-file -e "$ref:$file" 2>/dev/null; then
    echo "skip  $file — new in the working tree, sqlx will apply it normally" >&2
    continue
  fi

  before="$(git show "$ref:$file" | sql_only)"
  after="$(sql_only < "$file")"

  if [ "$before" != "$after" ]; then
    echo "REFUSE  $file — the SQL changed, not just the comments." >&2
    echo "        A migration that already ran cannot be rewritten; add a new one." >&2
    blocked=1
    continue
  fi

  sum="$(shasum -a 384 "$file" | cut -d' ' -f1)"
  statements+="UPDATE _sqlx_migrations SET checksum = UNHEX('$sum') WHERE version = $version;"$'\n'
  echo "ok    $file — comments only, version $version" >&2
  changed=$((changed + 1))
done < <(git diff --name-only "$ref" -- migrations/ ; git diff --cached --name-only "$ref" -- migrations/)

if [ "$blocked" -ne 0 ]; then
  echo >&2
  echo "Refused at least one migration, so NOTHING was emitted — a partial" >&2
  echo "re-blessing would leave the database agreeing with a file the binary" >&2
  echo "does not have. Revert the SQL change, or add a new migration." >&2
  exit 1
fi

if [ "$changed" -eq 0 ]; then
  echo "No migration files differ from $ref — nothing to re-bless." >&2
  exit 0
fi

printf '%s' "$statements"
