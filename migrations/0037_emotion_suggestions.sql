-- Life schema, migration 0037: remember the feelings a model suggested for a
-- check-in note, and queue the work that produces them.
--
-- Suggestions used to be computed inside the request: the picker asked, the
-- server called a model, and you watched a spinner for as long as that took —
-- every single time you opened the picker, even for a note whose suggestions had
-- already been worked out a minute earlier. Two things follow from caching them
-- instead.
--
-- First, `emotion_suggestions` holds ONE row per check-in: the latest tokens, and
-- the hash of the note text they were computed from. Reopening the picker on an
-- unchanged note is a table lookup. Editing the note makes the hash disagree,
-- which is exactly what marks the stored set as belonging to the *earlier*
-- wording — so it can still be shown, honestly labelled, while the new one is
-- computed. The row is keyed by the check-in's ulid rather than the note hash so
-- there is always something to fall back on: a per-hash cache would have nothing
-- to show the moment you changed a word.
--
-- Second, generation moved off the request path into `emotion_jobs`, because the
-- model runs on the Mac and the fleet may not dial the Mac (the WireGuard peer is
-- deliberately one-way). The Mac's worker polls for jobs and posts results back,
-- so the pod only ever accepts connections. At most one job per check-in: writing
-- a note is an edit-in-progress, and only the newest wording is worth computing.
--
-- Neither table references `wellbeing`: the picker can be opened on a check-in
-- that hasn't synced to the server yet, and a suggestion for a ulid the server
-- has never seen is still useful. Both are pure derived data — losing them costs
-- one recomputation, so neither is backed up or synced.

CREATE TABLE IF NOT EXISTS emotion_suggestions (
    user_id     VARCHAR(64) NOT NULL,
    -- The check-in these belong to (client-side ulid; no FK, see above).
    ulid        CHAR(26)    NOT NULL,
    -- SHA-256 (hex) of the note text these were computed from. Equal to the
    -- current note's hash → fresh; different → computed from an earlier wording.
    note_hash   CHAR(64)    NOT NULL,
    -- JSON array of `Core/Name` tokens, most-fitting first. '[]' is a real
    -- answer ("nothing in the list fits"), not a missing one.
    tokens      JSON        NOT NULL,
    computed_at DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, ulid)
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS emotion_jobs (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    user_id     VARCHAR(64) NOT NULL,
    ulid        CHAR(26)    NOT NULL,
    note_hash   CHAR(64)    NOT NULL,
    -- The whole prompt, built server-side (vocabulary + the user's own past
    -- taggings as few-shot + the note). Self-contained on purpose: the worker is
    -- a dumb generator that needs no access to the database.
    prompt      JSON        NOT NULL,
    -- The tokens that were on offer, so a model's answer can be validated
    -- against the vocabulary when it comes back rather than when it was asked.
    candidates  JSON        NOT NULL,
    -- When this wording was first queued — the honest answer to "how long has it
    -- been thinking", which survives closing and reopening the picker. Reset only
    -- when the note itself changes, not when the picker asks again.
    created_at  DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    -- When a worker claimed it; a claim older than the stale window is retried,
    -- so a worker that dies mid-job doesn't strand the note forever.
    taken_at    DATETIME    NULL,
    -- One pending job per check-in: a newer wording replaces the older one in
    -- place rather than queueing behind it.
    UNIQUE KEY uq_emotion_job_entry (user_id, ulid)
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
