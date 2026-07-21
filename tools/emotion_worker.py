#!/usr/bin/env python3
"""Generate emotion suggestions for life check-ins, on the machine that has the model.

The model is Apple-Silicon MLX and lives on the Mac; life runs on a fleet server
that is not allowed to open connections toward the Mac (it is a deliberately
one-way VPN peer, so a compromised server cannot reach the machine holding the
originals). So the direction is inverted: this worker dials *out* to life, asks
for work, generates, and posts the answer back.

It holds no vocabulary and no database. A job is a self-contained prompt; the
answer is the model's raw text. Deciding whether that text names real feelings
is life's job, where the vocabulary is known and the check is tested — a worker
that could widen its own answer set would be a hole in that guarantee.

The model is loaded on the first job and released after a stretch of quiet, so a
feature used a handful of times a day doesn't hold gigabytes of GPU memory all
day next to recall's transcription.

Environment:
  LIFE_URL              base URL of the life server (default https://life.xinutec.org)
  EMOTION_WORKER_TOKEN  shared secret; must match the server's (required)
  EMOTION_MODEL         MLX model id (default mlx-community/Qwen2.5-7B-Instruct-4bit)
  EMOTION_IDLE_UNLOAD   seconds of quiet before the model is released (default 300)

Needs an interpreter with `mlx-lm` installed. On this Mac that is recall's venv,
which already has both the library and this model cached:

  ~/Code/recall/.venv/bin/python tools/emotion_worker.py
"""

from __future__ import annotations

import json
import logging
import os
import time
import urllib.error
import urllib.request
from typing import Any

LOG = logging.getLogger("emotion-worker")

DEFAULT_MODEL = "mlx-community/Qwen2.5-7B-Instruct-4bit"
# The answer is a short JSON array of tokens. A tight bound keeps a model that
# starts rambling from pinning the GPU.
MAX_TOKENS = 128
# Longer than the server's poll window, so a held-open poll is never mistaken for
# a dead connection; short enough that a genuinely wedged one is noticed.
POLL_TIMEOUT = 40
# Backoff after a network failure. The server being down is normal (deploys), not
# an error worth spinning on.
RETRY_SECS = 10


def _request(url: str, token: str, *, method: str = "GET", body: dict[str, Any] | None = None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Authorization", f"Bearer {token}")
    if data is not None:
        req.add_header("Content-Type", "application/json")
    return urllib.request.urlopen(req, timeout=POLL_TIMEOUT)


def next_job(base: str, token: str) -> dict[str, Any] | None:
    """Long-poll for work. The server holds the request open, so this blocks."""
    with _request(f"{base}/api/emotion-worker/next", token) as resp:
        if resp.status == 204:
            return None
        return json.loads(resp.read())


def post_result(base: str, token: str, job_id: int, *, content: str | None, error: str | None):
    body = {"content": content} if error is None else {"error": error}
    with _request(
        f"{base}/api/emotion-worker/{job_id}/result", token, method="POST", body=body
    ) as resp:
        resp.read()


class Model:
    """The MLX model, loaded on demand and released when it goes unused.

    Five minutes of quiet is enough to let go of ~4.3 GB. Loading it back costs a
    few seconds off a warm page cache — paid on the first note after a lull, and
    invisible in practice because suggestions are cached: nobody sits waiting on
    this to read a check-in they just wrote. Holding the weights longer would buy
    speed nobody is waiting for, at recall's expense.
    """

    def __init__(self, name: str, idle_unload: float) -> None:
        self.name = name
        self.idle_unload = idle_unload
        self._loaded: tuple[Any, Any] | None = None
        self._last_used = 0.0

    def generate(self, system: str, user: str) -> str:
        from mlx_lm import generate, load  # heavy import stays lazy

        if self._loaded is None:
            LOG.info("loading %s", self.name)
            loaded = load(self.name)
            self._loaded = (loaded[0], loaded[1])
        llm, tokenizer = self._loaded
        prompt = tokenizer.apply_chat_template(
            [{"role": "system", "content": system}, {"role": "user", "content": user}],
            add_generation_prompt=True,
        )
        self._last_used = time.monotonic()
        text: str = generate(llm, tokenizer, prompt=prompt, max_tokens=MAX_TOKENS)
        self._last_used = time.monotonic()
        return text

    def release_if_idle(self) -> None:
        if self._loaded is None:
            return
        if time.monotonic() - self._last_used < self.idle_unload:
            return
        LOG.info("releasing %s after %.0fs idle", self.name, self.idle_unload)
        self._loaded = None
        import gc

        gc.collect()


def run(base: str, token: str, model: Model) -> None:
    while True:
        try:
            job = next_job(base, token)
        except urllib.error.HTTPError as e:
            # 401 is a configuration problem, not a blip: say so plainly rather
            # than retrying a wrong token forever in silence.
            LOG.error("poll failed: HTTP %s%s", e.code, " (check the token)" if e.code == 401 else "")
            time.sleep(RETRY_SECS)
            continue
        except (urllib.error.URLError, TimeoutError, OSError) as e:
            LOG.warning("poll failed: %s", e)
            time.sleep(RETRY_SECS)
            continue

        if job is None:
            model.release_if_idle()
            continue

        job_id = job["id"]
        prompt = job.get("prompt") or {}
        started = time.monotonic()
        try:
            content = model.generate(prompt.get("system", ""), prompt.get("user", ""))
        except Exception as e:  # a bad job must not take the worker down with it
            LOG.exception("job %s failed", job_id)
            _post_quietly(base, token, job_id, content=None, error=str(e))
            continue
        LOG.info("job %s answered in %.1fs", job_id, time.monotonic() - started)
        _post_quietly(base, token, job_id, content=content, error=None)


def _post_quietly(base: str, token: str, job_id: int, *, content: str | None, error: str | None):
    """Report an answer, tolerating a server that blinked. The work is lost, the
    note is not: it is queued again the next time the picker is opened."""
    try:
        post_result(base, token, job_id, content=content, error=error)
    except (urllib.error.URLError, TimeoutError, OSError) as e:
        LOG.warning("could not report job %s: %s", job_id, e)


def main() -> int:
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
    token = os.environ.get("EMOTION_WORKER_TOKEN", "")
    if not token:
        LOG.error("EMOTION_WORKER_TOKEN is not set")
        return 2
    base = os.environ.get("LIFE_URL", "https://life.xinutec.org").rstrip("/")
    model = Model(
        os.environ.get("EMOTION_MODEL", DEFAULT_MODEL),
        float(os.environ.get("EMOTION_IDLE_UNLOAD", "300")),
    )
    LOG.info("polling %s for emotion jobs", base)
    run(base, token, model)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
