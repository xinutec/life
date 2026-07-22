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

This worker does not load the model. The Mac holds exactly one copy of it, in
recall's `llm-host` daemon (recall/src/recall/llmhost.py, 127.0.0.1:8092), which
also serves recall's own summaries and Ask. Loading a second copy here would put
two ~4.3 GB models on a machine that is also transcribing; the holder loads on
demand, generates one request at a time, and releases the weights after five idle
minutes. So the model shows up in this file as an HTTP call.

Environment:
  LIFE_URL              base URL of the life server (default https://life.xinutec.org)
  EMOTION_WORKER_TOKEN  shared secret; must match the server's (required)
  EMOTION_MODEL         model id the holder should use
  EMOTION_LLM_HOST      where the holder listens (default http://127.0.0.1:8092)

Standard library only — any python3 will run it:

  python3 tools/emotion_worker.py
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
DEFAULT_LLM_HOST = "http://127.0.0.1:8092"
# The answer is a short JSON array of tokens. A tight bound keeps a model that
# starts rambling from pinning the GPU.
MAX_TOKENS = 128
# Longer than the server's poll window, so a held-open poll is never mistaken for
# a dead connection; short enough that a genuinely wedged one is noticed.
POLL_TIMEOUT = 40
# The holder may have to load the weights first (~60s cold) and it serialises
# callers, so a request can queue behind recall's. Nobody is waiting on this.
GENERATE_TIMEOUT = 300
# Backoff after a network failure. The server being down is normal (deploys), not
# an error worth spinning on.
RETRY_SECS = 10


class HolderDown(Exception):
    """The llm-host could not be reached — the work is not doable right now.

    Distinct from a job that genuinely failed: a failure is reported to life and
    cached as "no suggestions for this note", which would be a lie if the only
    problem is that a daemon is restarting. This one leaves the job queued.
    """


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
    """The model, addressed where it actually lives: recall's llm-host.

    Nothing is cached here. The holder decides when the weights are resident and
    when they go back, for every consumer at once — which is the only way the
    Mac can hold one copy rather than one per interested process.
    """

    def __init__(self, name: str, host: str) -> None:
        self.name = name
        self.host = host.rstrip("/")

    def generate(self, system: str, user: str) -> str:
        body = json.dumps(
            {
                "prompt": user,
                "system": system,
                "model": self.name,
                "max_tokens": MAX_TOKENS,
            }
        ).encode()
        req = urllib.request.Request(
            f"{self.host}/generate",
            data=body,
            method="POST",
            headers={"Content-Type": "application/json"},
        )
        try:
            with urllib.request.urlopen(req, timeout=GENERATE_TIMEOUT) as resp:
                answer: str = json.loads(resp.read())["text"]
        except urllib.error.HTTPError as e:
            # 5xx is the holder failing, not this job being unanswerable — a bug
            # in it (2026-07-22: a crash in its own logging) 500'd every job, and
            # reporting those cached real notes as having no feelings in them.
            # Defer instead, and let a retry sort it out once the holder is fixed.
            # Only a 4xx says something about the request itself.
            if e.code >= 500:
                raise HolderDown(f"llm-host is failing: HTTP {e.code}") from e
            raise RuntimeError(f"llm-host refused the job: HTTP {e.code}") from e
        except (urllib.error.URLError, TimeoutError, OSError) as e:
            raise HolderDown(f"no llm-host at {self.host}: {e}") from e
        return answer


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
            continue

        job_id = job["id"]
        prompt = job.get("prompt") or {}
        started = time.monotonic()
        try:
            content = model.generate(prompt.get("system", ""), prompt.get("user", ""))
        except HolderDown as e:
            # Say nothing to life: an unreported job stays queued and is claimed
            # again shortly, whereas reporting a failure would cache this note as
            # having no feelings in it. Wait for the daemon to come back.
            LOG.warning("job %s deferred: %s", job_id, e)
            time.sleep(RETRY_SECS)
            continue
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
        os.environ.get("EMOTION_LLM_HOST", DEFAULT_LLM_HOST),
    )
    LOG.info("polling %s for emotion jobs, generating via %s", base, model.host)
    run(base, token, model)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
