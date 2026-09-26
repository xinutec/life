# The emotion-suggestion worker as a store path for launchd (see
# deploy/hm-agents.nix); `packages.<system>.emotion-worker`. It is stdlib-only,
# so the runtime is plain python3. The token is read at runtime from
# ~/.config/life/worker.env, never the world-readable store.
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
