#!/usr/bin/env bash
# Run life's emotion-suggestion worker on the Mac (see tools/emotion_worker.py).
#
# Wrapper rather than a bare ProgramArguments line so the launchd agent has one
# stable entry point: the interpreter, the secret and the log directory are
# details that can change here without touching the plist (and therefore without
# a home-manager switch).
#
# The secret lives outside the repo and outside the nix store — both would
# publish it — in ~/.config/life/worker.env:
#
#   EMOTION_WORKER_TOKEN=…same value as the server's…
#
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p logs

ENV_FILE="${LIFE_WORKER_ENV:-$HOME/.config/life/worker.env}"
if [ ! -r "$ENV_FILE" ]; then
  echo "no worker env at $ENV_FILE — the worker has no token to authenticate with" >&2
  exit 78 # EX_CONFIG: a configuration problem, not a crash to restart into
fi
set -a
# shellcheck disable=SC1090  # deliberately a runtime path, not a fixed file
. "$ENV_FILE"
set +a

# recall's venv already has mlx-lm and this model cached; any interpreter with
# mlx-lm installed works.
PYTHON="${EMOTION_WORKER_PYTHON:-$HOME/Code/recall/.venv/bin/python}"
exec "$PYTHON" tools/emotion_worker.py
