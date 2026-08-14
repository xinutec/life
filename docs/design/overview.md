# life — architecture & scope

Personal single-user home OS. Owns an **inventory + spatial model** of the house,
a **product catalog** fed from shops and Open Food Facts, **recipes**,
**shopping**, **typed to-dos** and **wellbeing tracking** — and **delegates
scheduling to Nextcloud Calendar** rather than reinventing it.

The whole app lives at one origin, `life.xinutec.org` — one Angular app, one Rust
backend, one MariaDB. "Domain" here and in the code (the *inventory domain*)
means a bounded context, not a DNS name; there are no per-feature subdomains.

Read alongside:

- [`sync.md`](sync.md) — the local-first sync protocol and its traps. Read it
  before touching `src/sync/` or `frontend/src/app/sync/`.
- [`catalog-and-holdings.md`](catalog-and-holdings.md) — products vs items,
  shop listings, per-source facts and reconciliation.
- [`ui-grammar.md`](ui-grammar.md) — the interaction rules every screen follows.
- [`../TODO.md`](../TODO.md) — what's built, what's next, and the open decisions.

---

## 1. Shape

Rust + axum, Angular (Material 3), MariaDB via `sqlx`, three.js for the house;
isis k3s, namespace `life`; the DB dump rides in the Mac mini's existing restic
set. Each choice is "same as `home` / `health` / `recall`" — one set of fleet ops
beats a locally optimal stack.

Nextcloud is **not** the database. It is touched at two boundaries only:
identity and calendar.

> **Firm boundary:** the *only* thing life manages through NC is the **calendar**.
> Inventory, locations, recipes, history and wellbeing live in life's own
> MariaDB. "No NC DB writes" means *no schema surgery* — public APIs are fine, so
> writing a `VEVENT` over CalDAV is allowed; touching NC's app tables is not.

## 2. Nextcloud integration — why there are two flows

life runs two deliberately separate NC flows, for the reasons `health` learned
the hard way (`kubes/health/src/nextcloud/*`).

**2a. Identity — OAuth2 authorization-code, identity-only.** Exchange the code,
call `/ocs/v2.php/cloud/user` once for `{id, displayname}`, then **discard the
tokens** and mint life's own session (§3). Because no NC **refresh** token is
ever held, life never hits the single-use-refresh-token rotation race that forced
health to split its flows.

**2b. Calendar — app password (NC Login Flow v2) → CalDAV.** Identity-OAuth2
cannot reach the DAV endpoints, so calendar read/write needs a second flow
yielding a long-lived app password (no expiry, HTTP Basic), stored in life's own
`nc_credentials`. CalDAV is then plain authenticated HTTP against
`/remote.php/dav/calendars/<user>/…`.

## 3. Sessions — life's own, DB-backed

NC is touched only at login; every later request authenticates against life's own
opaque session (random id → `sessions` row; cookie is `id.HMAC`, verified
timing-safe). Two decisions worth keeping in mind:

- **The pending login travels in a signed cookie of ours, not keyed by `state`.**
  When the browser holds no NC session, NC's `oauth2/authorize` bounces to its
  own Login Flow and drops every query parameter, so `state` comes back
  **empty** — a server that looks the pending login up by `state` cannot complete
  a login from a cookie-less browser at all. Every app in this family shares the
  flow and the fix; the full reasoning, including the accepted residual risk, is
  in `src/pending_login.rs`.
- **`return_to` only accepts same-site internal paths**, rejecting `//host` and
  `/\host` — browsers fold `\` to `/`, so `/\evil.com` redirects off-site. This
  is the open-redirect guard, not cosmetic.

## 4. Data model

The core insight: cupboard containment is **general asset tracking**. One generic
engine, with food/recipes as the first skin — meds, tools and documents later
become a category and a few fields, not a new app.

### Containment (location graph)

```
item → layer → cupboard → room → house
```

`location` is a node with a `kind` and a parent, so registering a cupboard is one
insert. A `layer` is a cupboard's ordered child.

**3D geometry lives in `scenes/house.json`, not on the location rows.** Boxes are
centre-based — `{cx, cz, w, d, h, y0}` — in **metres**, floor plane **X–Z**,
**Y up**. Whether that geometry should move onto `location` rows (versus staying
separate and mapping by id/name) is an open decision in
[`../TODO.md`](../TODO.md); until it's settled, a room in a `todo_link` is
referenced by *name*, because that's the only identity the scene has.

### Item

Generic from day one: `category` is not hard-coded to food, and `expiry` is
first-class rather than a food-only afterthought. **History is recorded from the
start** because it cannot be backfilled.

### To-do — typed tasks over a connection graph

Not a flat checklist. `todo` carries a **curated `type` enum** that grows one
variant at a time as real kinds appear (`purchase`, `call`, …) rather than being
guessed up front.

`todo_link` is a **typed, directional** edge — `depends-on | subtask | related` —
to another to-do or to an app entity (`item` / `recipe` / `shopping` / `place` by
id, or a house `room` by *name*, since rooms live in `scenes/house.json`, not the
DB). Modelling connections as edge rows rather than baking relationships into the
to-do keeps every kind of link uniform and queryable from either end.

Timing extends actionability instead of sitting beside it as a date column:

- **`not_before`** → the to-do is **waiting** — the temporal analogue of
  *blocked*, and also how snoozing works ("not this week").
- **`due`** → **urgency**, an axis orthogonal to actionability. A to-do can be
  blocked *and* due tomorrow, which is the state most worth surfacing.
- Precedence `done → blocked → waiting → ready/open`: when an item is both
  dep-blocked and deferred, the external gate is the informative one.

Both are **dates, not datetimes**. Hour-level scheduling stays in NC Calendar
(§5); these fields are list-ordering and attention metadata, not a scheduler.

Deliberately not built: lead-time estimates (they decay into noise), soft-vs-hard
deadlines (a taxonomy nobody maintains), recurrence (NC does it).

### Wellbeing

An **entry**, not a daily value: `(recorded_at, score, optional energy, emotions,
note)`. Several entries a day is the point — "down in the morning, good in the
afternoon" is two entries. No streaks, no gamification, no prompts; capture is
always user-initiated, and the design constraint is that logging a mood costs one
tap or it stops happening. Backdating exists so "this morning I felt down" can be
logged at 15:00.

Readings are stored in **tenths** (`score_tenths`, `energy_tenths`; 10..50, where
35 is a 3.5) — fixed-point integers so they average and compare exactly. The
columns were *renamed* when rescaled so stale code fails to find them rather than
plotting a 4 as a 0.4; see `migrations/0023_wellbeing_tenths.sql`. Energy is
stored higher-is-better and displayed as its complement, "fatigue" — the
inversion is display-only.

## 5. Scheduling — delegated to NC Calendar

Shop trips and reminders live in NC Calendar, not a life table. life **writes**
"go to <shop>" `VEVENT`s (free-text `LOCATION` always works; geocoded `GEO`
depends on the NC version — verify before relying on it) and **reads** the Brent
bins subscription as an *input*, e.g. don't schedule a shop the morning the bins
go out. This shows up in every calendar client for free and keeps a whole
scheduling subsystem out of life's DB.
