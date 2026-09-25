# Packages the emotion-suggestion worker so launchd runs a fixed store path
# instead of a script inside the working tree. Referenced by
# deploy/hm-agents.nix and exposed as `packages.<system>.emotion-worker`.
#
# The worker asks recall's llm-host to generate, so it imports only the
# standard library: the runtime is just a python3.
#
# The token deliberately does NOT come from the store (which is world-readable):
# the wrapper reads ~/.config/life/worker.env at runtime.
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
