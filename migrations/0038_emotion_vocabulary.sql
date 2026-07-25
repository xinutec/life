-- Life schema, migration 0038: remember the feelings vocabulary the picker last
-- offered, so the day's prompt can be rebuilt with nobody waiting on it.
--
-- The vocabulary lives in the FRONTEND (the emotion wheel), and is sent with each
-- request on purpose: one source of truth, no second copy on the server to drift
-- from the wheel you actually see. This table does not change that — it is a
-- cache of what the client last declared, never an authority. Anything sent later
-- replaces it, and if it is empty or stale the only consequence is that a
-- preload is skipped or wasted.
--
-- Why it has to be persisted at all: the day's few-shot is cut off at the end of
-- yesterday (see `suggest::fetch_examples`), so the whole system prompt — and
-- therefore the model's KV-cache prefix — changes at the UTC rollover. Rebuilding
-- that cache costs ~50s of prefill on top of a ~60s cold model load, and until
-- now the first check-in of each day paid for both while you sat waiting. A timer
-- just after midnight does it instead, but a timer has no request to read the
-- vocabulary from — hence this row. Persisted rather than held in memory so a pod
-- restart doesn't silently put the slow day back.
--
-- Derived data: losing it costs one un-warmed morning, so it is neither backed up
-- nor synced.

CREATE TABLE IF NOT EXISTS emotion_vocabulary (
    user_id    VARCHAR(64) NOT NULL PRIMARY KEY,
    -- JSON array of {token, desc} exactly as the picker sent it, in its order —
    -- the prompt is built from this verbatim, and order changes the bytes, which
    -- changes the cache key.
    candidates JSON        NOT NULL,
    updated_at DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP
                           ON UPDATE CURRENT_TIMESTAMP
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
