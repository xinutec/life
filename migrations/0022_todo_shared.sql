-- A to-do is `shared` when it belongs on the case-file site (life-todo-sync
-- checks it against the case-file markdown); everything else is private and
-- app-only. Default is PRIVATE — publishing to the case file is a deliberate,
-- explicit act, the safe default for a medical case file.
--
-- Backfill: the existing case-file to-dos already carry a `Source: <file>.md`
-- line in their notes (hand-entered when mirroring a case-file checkbox), so
-- that provenance marks them shared exactly once here. Everything without it —
-- personal to-dos added in the app — stays private. After this, the explicit
-- `shared` flag is the source of truth and the Source line is only the match key.
ALTER TABLE todos ADD COLUMN shared BOOLEAN NOT NULL DEFAULT FALSE;
UPDATE todos SET shared = TRUE WHERE notes LIKE '%Source:%';
