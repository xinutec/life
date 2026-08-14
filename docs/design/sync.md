# life — local-first sync

**As built.** RxDB in the browser, MariaDB behind axum, replication handlers we
own. The local store is the source of truth: the UI reads and writes locally and
never branches on connectivity; sync is a background concern.

Four collections sync — **shopping, todo, todo-link, wellbeing**. `items`,
`locations` and `recipes` are **online-only REST by choice**: inventory is edited
at home on wifi, so the per-table sync cost isn't yet worth it. The pattern below
applies unchanged when that changes.

## Why this shape

**RxDB, not hand-rolled.** Correctness is the constraint, not cost. Checkpoints,
tombstones, partial-failure replay, retry/backoff, conflict hooks and ordering
are what is subtly wrong when hand-built. RxDB's replication is designed
for a custom backend — implement a pull and a push handler, it drives the rest —
and it is datastore-agnostic, so MariaDB stays.

**RxDB over the Postgres engines.** ElectricSQL and Zero exist for
server-authoritative, permission-scoped fan-out to many clients. One user with an
effectively serial write stream has none of those problems, and neither engine
owns the hard part: Electric is read-path only, and Zero's mutators run as
TypeScript on its own server, which would fork write authority out of axum. RxDB
also replicates over plain same-origin `fetch`, so the NC session cookie rides
along with no new auth surface. **PowerSync is the recorded fallback** if owning
the handlers ever gets heavy: mature, managed, and it speaks MariaDB, so it
needs no migration.

**No CRDTs.** One user, one active device at a time. Conflict handling is a
**field-level 3-way merge** (`frontend/src/app/sync/conflict-merge.ts`) with the
client's assumed master as base, so non-overlapping edits from two devices both
survive; a same-field collision keeps the pushing device's value and reports the
loser to the conflict log (`/api/conflicts`, with a keep-mine / use-other screen).

## Protocol

Checkpoint-based pull + push, one stream per collection, generic over the
document type (`PullResponse<D>` / `PushEntry<D>` in `src/sync/types.rs`); the
per-collection part is the doc shape plus its repo functions.

- **Pull** `GET /api/sync/{collection}?since={checkpoint}` — rows including
  tombstones with `rev >` the checkpoint, ordered by `rev`.
- **Push** `POST /api/sync/{collection}` — `{newDocumentState, assumedMasterState}`
  applied as an idempotent upsert keyed by ULID, but only if the server's `rev`
  matches; on mismatch the current server doc comes back as a conflict. Replaying
  a push after a flaky connection is a no-op.

Every syncable row carries a client-minted `ulid` (the stable identity — the
`BIGINT` PK stays internal), a server-assigned `rev`, and `deleted_at`.

## The rules that are load-bearing

Each of these was a bug or a near-miss. Breaking one is silent.

- **`rev` is assigned at commit, not at write time.** A sequence handed out
  mid-transaction can commit out of order (6 visible before 5); a pull in that
  window advances the checkpoint past 5 and never delivers it.
- **Tombstones are set-only on the server.** A push can never clear
  `deleted_at`. The one deliberate undelete is the trash restore
  (`/api/trash/{kind}/{ref}/restore`), which bumps `rev` so the resurrection
  propagates.
- **Cross-row links are soft at sync time.** Collections replicate on
  independent streams, so a child can arrive before its parent. Syncable rows
  reference each other **by ULID**; hard `BIGINT` FKs are never enforced against
  sync input.
- **The push boundary validates and rejects — it does not clamp.** Clamping is a
  masking fallback. An out-of-range reading or an unknown enum is a 400, and a
  batch containing one bad doc applies nothing (the push is one transaction).
- **Auth expiry is not a clean 401.** An expired NC session yields a 302 to login
  HTML that `fetch` follows to a 200. Worse, the Angular service worker answers a
  failed fetch with a bodiless synthetic **504**, so "not JSON" alone cannot mean
  "logged out" — that heuristic once signed the user out on every offline launch
  (see `../TODO.md`, 2026-07-16). On real auth loss, replication stands down
  rather than retrying forever.
- **RxDB schema versions are as load-bearing as `migrations/`.** A version bump
  without a `migrationStrategy` fails to open the local DB and can drop unsynced
  writes. Every schema change ships a strategy; never a silent wipe.
- **Nothing is ever purged.** Tombstones stay forever (`src/trash/mod.rs`), which
  is what makes the trash screen work *and* what keeps a long-offline client from
  missing a delete and re-creating the row. Before adding any purge, add the
  guard first: a checkpoint older than the purge horizon must force a full
  re-sync.

## Known hazards, not yet live

- **Quantity is the one field LWW cannot own.** It has *accumulate* semantics:
  two offline edits (12→10, 12→9) keep one absolute value and silently drop the
  other adjustment. `items` is online-only today, so this is dormant — model
  quantity as deltas (the `item_history` append-log is the natural carrier)
  before inventory joins sync.
- **Compound operations aren't atomic across streams.** `buy` (create item +
  delete shopping row) and friends must be idempotent, replay-safe ULID-keyed
  pairs once both sides are synced.

## Offline cold start

The Angular service worker precaches the shell, and
`frontend/e2e/offline-boot.spec.ts` pins the invariant that a **signed-in app
opened offline stays signed in** — the test exists because ngsw answers a failed
fetch with a synthetic 504, which the auth guard once read as a logout.

That is proven **in the browser**. The Android wrapper is a plain `WebView`
loading the site over the network, with no `ServiceWorkerController` wiring, and
a WebView can fail a top-level navigation before any SW intercept. So offline
cold start on the phone is unverified, not established. If it turns out broken,
the fix on the table is bundling the shell into the APK and letting the SW cache
only subresources and data.

Data at rest is plaintext IndexedDB. Inventory and wellbeing are low-sensitivity
and RxDB encryption is a paid plugin — a stated decision, not an omission.

---

**Migrations don't cite this file, on purpose.** `sqlx::migrate!()` checksums
every applied migration, so a migration's comments are effectively frozen — and
a frozen file that names a path in a living doc tree is a dangling reference
waiting to happen. Migration headers state their reasoning inline instead.

When a migration comment genuinely has to change, `scripts/rechecksum-migrations.sh`
is the supported route: it re-blesses a migration only when the SQL is untouched,
and emits nothing at all if any file fails that test. Apply its statements to
every database that already ran the migration, in the same breath as deploying
the matching image — a database and a binary that disagree will not boot.
