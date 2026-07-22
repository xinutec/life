# Packages the emotion-suggestion worker so launchd runs a fixed store path
# instead of a script inside the working tree. Referenced by
# deploy/hm-agents.nix and exposed as `packages.<system>.emotion-worker`.
#
# Why this one is packageable at all: since it asks recall's llm-host to
# generate (tools/emotion_worker.py), it imports nothing but the standard
# library — so "the runtime" is just a python3, and the whole thing is one file.
# The heavy MLX stack stays where it belongs, in recall's uv venv.
#
# The token deliberately does NOT come from the store (which is world-readable):
# the wrapper reads ~/.config/life/worker.env at runtime, the same file the old
# shell wrapper sourced.
{ python3, writeShellApplication }:

writeShellApplication {
  name = "life-emotion-worker";
  runtimeInputs = [ python3 ];
  text = ''
    ENV_FILE="''${LIFE_WORKER_ENV:-$HOME/.config/life/worker.env}"
    if [ ! -r "$ENV_FILE" ]; then
      echo "no worker env at $ENV_FILE — the worker has no token to authenticate with" >&2
      exit 78 # EX_CONFIG: a configuration problem, not a crash to restart into
    fi
    set -a
    # shellcheck disable=SC1090  # deliberately a runtime path, not a fixed file
    . "$ENV_FILE"
    set +a

    exec python3 ${../tools/emotion_worker.py} "$@"
  '';
}
