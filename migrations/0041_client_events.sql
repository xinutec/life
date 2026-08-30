-- Life schema, migration 0041: keep the client's activity trace instead of only
-- logging it.
--
-- `/api/telemetry` folded the browser's navigations and taps into the backend
-- log and forgot them. That is the right shape for *reading a session* — the
-- client events interleave with the per-request trace, so a timeline
-- reconstructs — but it makes the record as short-lived as one pod's log
-- buffer. Measured 2026-08-30: 28 hours and 12 events, erased by a restart.
--
-- The question that needs the history is "what should the daily surfaces do
-- next". The wellbeing table already answers the *capture* question (an
-- unbroken daily habit, completion rising), and precisely because it is
-- unbroken it says nothing about what to build: a feature that was never built
-- leaves no trace in it. The trace of what a person actually did on the way to
-- each entry is the only place that evidence can come from, and it is only
-- evidence if it accumulates.
--
-- The log line stays. This table is not a replacement for it — reading one
-- session is still far easier in the interleaved stream, and losing that would
-- be a real cost. Both, deliberately.
--
-- Two clocks, kept apart on purpose:
--
--   `client_at_ms` is the browser's own clock, stored verbatim as the client
--   sent it. A batch lands all at once, so the server's receive time cannot
--   order events *within* it; the client's can. Stored as raw epoch millis
--   rather than converted to a DATETIME because the client clock is untrusted
--   input — converting it here would quietly assert it is right, and a skewed
--   or hostile clock would then be indistinguishable from a true one.
--
--   `received_at` is this server's clock, which is trustworthy for "when did
--   this actually arrive" and is what any per-day rollup should group by.
--
-- Keeping both is what makes skew *detectable* rather than hidden: the two
-- disagreeing is the signal, and it cannot be recovered from either alone.
--
-- No FK to any user table and no sync bookkeeping: this is an append-only
-- observation log, never edited, never synced to the client, and safe to drop
-- wholesale. Volume is small enough that no retention job is worth writing yet
-- — at the observed rate a year is a few thousand rows — but nothing here
-- prunes, so that assumption is the thing to re-check if it ever gets loud.

CREATE TABLE IF NOT EXISTS client_events (
    id           BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
    user_id      VARCHAR(255)    NOT NULL,
    -- "nav" (a route change) or "tap" (a control). Not an ENUM: a client sending
    -- a kind this server has not heard of is data about a version skew, and an
    -- ENUM would turn that into a failed insert that loses the whole batch.
    kind         VARCHAR(16)     NOT NULL,
    path         VARCHAR(512)    NOT NULL,
    -- Verbatim UI text, already flattened to one line. '' when the event has
    -- none (a nav), which is distinct from a tap on an unlabelled control.
    label        VARCHAR(160)    NOT NULL,
    client_at_ms BIGINT          NOT NULL,
    received_at  DATETIME(3)     NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    -- The two questions asked of this table: one person's session in order, and
    -- how often a given screen or control is reached.
    INDEX idx_client_events_user_time (user_id, received_at),
    INDEX idx_client_events_path (path)
) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
