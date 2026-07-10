//! Persistence for the offline-first sync protocol. The revision counter is
//! shared by every syncable table; the pull/push protocol bodies are written
//! once, generic over the per-collection [`SyncSpec`], so the safety rules —
//! FOR UPDATE row lock, rev guard, **set-only tombstone**, commit-ordered
//! revs, validate-before-write — cannot drift between collections. A new
//! collection implements the spec and gets the tested protocol for free.
//! See `docs/proposals/offline-first.md`.

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use sqlx::mysql::MySqlArguments;
use sqlx::query::Query;
use sqlx::{AssertSqlSafe, MySql, MySqlConnection, MySqlPool};
use ulid::Ulid;

use crate::error::AppError;
use crate::todo::types::{LinkKind, TargetKind, TodoPriority, TodoStatus, TodoType};

use super::types::{
    Checkpoint, PullResponse, PushEntry, ShoppingDoc, TodoDoc, TodoLinkDoc, WellbeingDoc,
};

/// Allocate the next global revision, **inside the caller's transaction**. The
/// `LAST_INSERT_ID(val + 1)` trick bumps and returns the counter atomically; the
/// row lock it takes is held until the caller commits, so revisions are handed out
/// in *commit* order — a pull can never advance past a rev that is assigned but not
/// yet committed (review S1). Must run on the same connection as the write it
/// stamps.
pub async fn next_rev(conn: &mut MySqlConnection) -> sqlx::Result<u64> {
    let res = sqlx::query("UPDATE sync_rev SET val = LAST_INSERT_ID(val + 1) WHERE id = 1")
        .execute(&mut *conn)
        .await?;
    Ok(res.last_insert_id())
}

// ---- the protocol, once -------------------------------------------------------

/// A push failure splits invalid client input (→ 400 at the route) from
/// everything else (→ 500). Invalid docs are rejected at the boundary — not
/// clamped, not stored — so a bad value can never poison later reads.
#[derive(Debug, thiserror::Error)]
pub enum PushError {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<PushError> for AppError {
    fn from(e: PushError) -> Self {
        match e {
            PushError::Invalid(msg) => AppError::BadRequest(msg),
            PushError::Db(e) => AppError::Other(e.into()),
            PushError::Other(e) => AppError::Other(e),
        }
    }
}

type DataQuery<'q> = Query<'q, MySql, MySqlArguments>;

/// Everything collection-specific about sync: the table, the data columns (in
/// bind order), the row↔doc mapping, and the push-boundary validation.
trait SyncSpec {
    type Doc: Send;
    type Row: for<'r> sqlx::FromRow<'r, sqlx::mysql::MySqlRow> + Send + Unpin;

    const TABLE: &'static str;
    /// Data columns beyond the protocol's own (id, ulid, deleted, rev), in the
    /// exact order `bind_data` binds their values.
    const DATA_COLS: &'static [&'static str];

    fn row_rev(row: &Self::Row) -> u64;
    fn row_doc(row: Self::Row) -> Result<Self::Doc>;
    fn ulid(doc: &Self::Doc) -> &str;
    fn rev(doc: &Self::Doc) -> u64;
    fn deleted(doc: &Self::Doc) -> bool;

    /// Reject values the typed REST boundary could not read back (unknown enum
    /// strings, out-of-range scores). Reject, not clamp — a clamp would be a
    /// masking fallback.
    fn validate(doc: &Self::Doc) -> Result<(), String>;

    /// Bind the `DATA_COLS` values, in that order.
    fn bind_data<'q>(q: DataQuery<'q>, doc: &'q Self::Doc) -> DataQuery<'q>;

    /// Insert-time hook: land the fresh row already tombstoned (used by the
    /// to-do-link twin dedupe). Default: never.
    async fn tombstone_on_insert(
        _tx: &mut MySqlConnection,
        _user_id: &str,
        _doc: &Self::Doc,
    ) -> sqlx::Result<bool> {
        Ok(false)
    }
}

fn select_list<C: SyncSpec>() -> String {
    // A boolean SQL expression decodes as an integer, so the row types map the
    // tombstone explicitly via the CAST alias.
    format!(
        "id, ulid, {}, CAST(deleted_at IS NOT NULL AS SIGNED) AS deleted, rev",
        C::DATA_COLS.join(", ")
    )
}

/// Pull: documents (including tombstones) with `rev` past the checkpoint, in rev
/// order, plus the advanced checkpoint. Scoped to one user.
async fn pull<C: SyncSpec>(
    pool: &MySqlPool,
    user_id: &str,
    since: u64,
    limit: u64,
) -> Result<PullResponse<C::Doc>> {
    let sql = format!(
        "SELECT {} FROM {} WHERE user_id = ? AND rev > ? ORDER BY rev ASC LIMIT ?",
        select_list::<C>(),
        C::TABLE
    );
    // The SQL is assembled from compile-time constants only (table + column
    // names off the spec); every runtime value is a bind parameter.
    let rows: Vec<C::Row> = sqlx::query_as(AssertSqlSafe(sql.as_str()))
        .bind(user_id)
        .bind(since)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    let checkpoint = Checkpoint {
        rev: rows.last().map_or(since, C::row_rev),
    };
    Ok(PullResponse {
        documents: rows.into_iter().map(C::row_doc).collect::<Result<_>>()?,
        checkpoint,
    })
}

/// Push: apply each change as an idempotent upsert keyed by ULID, guarded by the
/// client's assumed revision (optimistic concurrency). Returns the current server
/// doc for every rejected (stale) change so the client can resolve and re-push —
/// the LWW policy lives in the client's conflict handler; the server only enforces
/// the rev guard.
async fn push<C: SyncSpec>(
    pool: &MySqlPool,
    user_id: &str,
    entries: Vec<PushEntry<C::Doc>>,
) -> Result<Vec<C::Doc>, PushError> {
    // Validate the whole batch before writing anything: each entry gets its own
    // transaction below, so a mid-loop rejection would partially apply the
    // push. One bad doc → the whole request is a 400 and nothing is stored.
    for entry in &entries {
        C::validate(&entry.new_document_state).map_err(PushError::Invalid)?;
    }

    let select_sql = format!(
        "SELECT {} FROM {} WHERE ulid = ? AND user_id = ? FOR UPDATE",
        select_list::<C>(),
        C::TABLE
    );
    let update_sql = format!(
        "UPDATE {} SET {}, deleted_at = COALESCE(deleted_at, IF(?, NOW(), NULL)), \
         rev = ?, updated_at = NOW() WHERE ulid = ? AND user_id = ?",
        C::TABLE,
        C::DATA_COLS
            .iter()
            .map(|c| format!("{c} = ?"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let insert_sql = format!(
        "INSERT INTO {} (user_id, ulid, {}, deleted_at, rev, created_at, updated_at) \
         VALUES (?, ?, {}, IF(?, NOW(), NULL), ?, NOW(), NOW())",
        C::TABLE,
        C::DATA_COLS.join(", "),
        vec!["?"; C::DATA_COLS.len()].join(", ")
    );

    let mut conflicts = Vec::new();
    for entry in entries {
        let new = entry.new_document_state;
        let assumed_rev = entry.assumed_master_state.as_ref().map(C::rev);

        let mut tx = pool.begin().await?;
        // Lock this user's row (if any) for the rest of the transaction.
        let current: Option<C::Row> = sqlx::query_as(AssertSqlSafe(select_sql.as_str()))
            .bind(C::ulid(&new))
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;

        if let Some(cur) = current {
            if assumed_rev != Some(C::row_rev(&cur)) {
                conflicts.push(C::row_doc(cur)?);
                continue;
            }
            let rev = next_rev(&mut tx).await?;
            // Tombstones are SET-ONLY here: once deleted_at is set, no push can
            // clear it — a stale offline client must not silently resurrect a
            // deliberate delete. The one undelete path is the explicit trash
            // restore (see trash::repo), which is its own deliberate operation.
            C::bind_data(sqlx::query(AssertSqlSafe(update_sql.as_str())), &new)
                .bind(C::deleted(&new))
                .bind(rev)
                .bind(C::ulid(&new))
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
        } else {
            let tombstoned = C::tombstone_on_insert(&mut tx, user_id, &new).await?;
            let rev = next_rev(&mut tx).await?;
            C::bind_data(
                sqlx::query(AssertSqlSafe(insert_sql.as_str()))
                    .bind(user_id)
                    .bind(C::ulid(&new)),
                &new,
            )
            .bind(C::deleted(&new) || tombstoned)
            .bind(rev)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
    }
    Ok(conflicts)
}

// ---- shopping ---------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ShoppingDocRow {
    id: u64,
    ulid: String,
    name: String,
    quantity: Option<f64>,
    unit: Option<String>,
    barcode: Option<String>,
    done: bool,
    deleted: i64,
    rev: u64,
}

struct Shopping;

impl SyncSpec for Shopping {
    type Doc = ShoppingDoc;
    type Row = ShoppingDocRow;

    const TABLE: &'static str = "shopping_items";
    const DATA_COLS: &'static [&'static str] = &["name", "quantity", "unit", "barcode", "done"];

    fn row_rev(row: &ShoppingDocRow) -> u64 {
        row.rev
    }

    fn row_doc(r: ShoppingDocRow) -> Result<ShoppingDoc> {
        Ok(ShoppingDoc {
            ulid: r.ulid,
            id: Some(r.id),
            name: r.name,
            quantity: r.quantity,
            unit: r.unit,
            barcode: r.barcode,
            done: r.done,
            deleted: r.deleted != 0,
            rev: r.rev,
        })
    }

    fn ulid(doc: &ShoppingDoc) -> &str {
        &doc.ulid
    }

    fn rev(doc: &ShoppingDoc) -> u64 {
        doc.rev
    }

    fn deleted(doc: &ShoppingDoc) -> bool {
        doc.deleted
    }

    /// Free-form fields only — nothing the typed boundary re-parses.
    fn validate(_doc: &ShoppingDoc) -> Result<(), String> {
        Ok(())
    }

    fn bind_data<'q>(q: DataQuery<'q>, doc: &'q ShoppingDoc) -> DataQuery<'q> {
        q.bind(&doc.name)
            .bind(doc.quantity)
            .bind(&doc.unit)
            .bind(&doc.barcode)
            .bind(doc.done)
    }
}

/// One-time backfill: give every pre-sync shopping row a ULID + revision so it is
/// pulled by clients on first sync. Idempotent — only touches rows whose `ulid` is
/// still NULL, so it is a cheap no-op once done; safe to run on every boot.
pub async fn backfill_shopping(pool: &MySqlPool) -> Result<u64> {
    let mut total = 0u64;
    loop {
        let ids: Vec<(u64,)> =
            sqlx::query_as("SELECT id FROM shopping_items WHERE ulid IS NULL LIMIT 200")
                .fetch_all(pool)
                .await?;
        if ids.is_empty() {
            break;
        }
        for (id,) in ids {
            let mut tx = pool.begin().await?;
            let rev = next_rev(&mut tx).await?;
            sqlx::query(
                "UPDATE shopping_items SET ulid = ?, rev = ?, updated_at = NOW() \
                 WHERE id = ? AND ulid IS NULL",
            )
            .bind(Ulid::new().to_string())
            .bind(rev)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            total += 1;
        }
    }
    Ok(total)
}

pub async fn pull_shopping(
    pool: &MySqlPool,
    user_id: &str,
    since: u64,
    limit: u64,
) -> Result<PullResponse<ShoppingDoc>> {
    pull::<Shopping>(pool, user_id, since, limit).await
}

pub async fn push_shopping(
    pool: &MySqlPool,
    user_id: &str,
    entries: Vec<PushEntry<ShoppingDoc>>,
) -> Result<Vec<ShoppingDoc>, PushError> {
    push::<Shopping>(pool, user_id, entries).await
}

// ---- to-do ------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct TodoDocRow {
    id: u64,
    ulid: String,
    title: String,
    todo_type: String,
    status: String,
    priority: Option<String>,
    notes: Option<String>,
    not_before: Option<NaiveDate>,
    due: Option<NaiveDate>,
    shared: bool,
    deleted: i64,
    rev: u64,
}

struct Todo;

impl SyncSpec for Todo {
    type Doc = TodoDoc;
    type Row = TodoDocRow;

    const TABLE: &'static str = "todos";
    const DATA_COLS: &'static [&'static str] = &[
        "title",
        "todo_type",
        "status",
        "priority",
        "notes",
        "not_before",
        "due",
        "shared",
    ];

    fn row_rev(row: &TodoDocRow) -> u64 {
        row.rev
    }

    fn row_doc(r: TodoDocRow) -> Result<TodoDoc> {
        Ok(TodoDoc {
            ulid: r.ulid,
            id: Some(r.id),
            title: r.title,
            todo_type: r.todo_type,
            status: r.status,
            priority: r.priority,
            notes: r.notes,
            not_before: r.not_before,
            due: r.due,
            shared: r.shared,
            deleted: r.deleted != 0,
            rev: r.rev,
        })
    }

    fn ulid(doc: &TodoDoc) -> &str {
        &doc.ulid
    }

    fn rev(doc: &TodoDoc) -> u64 {
        doc.rev
    }

    fn deleted(doc: &TodoDoc) -> bool {
        doc.deleted
    }

    /// The enums ride as raw strings (the row shape); anything the typed REST
    /// boundary (`todo::repo`'s `TryFrom`) could not parse back is rejected
    /// here instead of 500ing the whole list on a later read.
    fn validate(doc: &TodoDoc) -> Result<(), String> {
        doc.todo_type.parse::<TodoType>()?;
        doc.status.parse::<TodoStatus>()?;
        if let Some(p) = &doc.priority {
            p.parse::<TodoPriority>()?;
        }
        Ok(())
    }

    fn bind_data<'q>(q: DataQuery<'q>, doc: &'q TodoDoc) -> DataQuery<'q> {
        q.bind(&doc.title)
            .bind(&doc.todo_type)
            .bind(&doc.status)
            .bind(&doc.priority)
            .bind(&doc.notes)
            .bind(doc.not_before)
            .bind(doc.due)
            .bind(doc.shared)
    }
}

pub async fn pull_todo(
    pool: &MySqlPool,
    user_id: &str,
    since: u64,
    limit: u64,
) -> Result<PullResponse<TodoDoc>> {
    pull::<Todo>(pool, user_id, since, limit).await
}

pub async fn push_todo(
    pool: &MySqlPool,
    user_id: &str,
    entries: Vec<PushEntry<TodoDoc>>,
) -> Result<Vec<TodoDoc>, PushError> {
    push::<Todo>(pool, user_id, entries).await
}

// ---- to-do links ------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct TodoLinkDocRow {
    id: u64,
    ulid: String,
    from_ulid: String,
    kind: String,
    target_kind: String,
    target_ref: String,
    deleted: i64,
    rev: u64,
}

struct TodoLink;

impl SyncSpec for TodoLink {
    type Doc = TodoLinkDoc;
    type Row = TodoLinkDocRow;

    const TABLE: &'static str = "todo_links";
    const DATA_COLS: &'static [&'static str] = &["from_ulid", "kind", "target_kind", "target_ref"];

    fn row_rev(row: &TodoLinkDocRow) -> u64 {
        row.rev
    }

    fn row_doc(r: TodoLinkDocRow) -> Result<TodoLinkDoc> {
        Ok(TodoLinkDoc {
            ulid: r.ulid,
            id: Some(r.id),
            from: r.from_ulid,
            kind: r.kind,
            target_kind: r.target_kind,
            target_ref: r.target_ref,
            deleted: r.deleted != 0,
            rev: r.rev,
        })
    }

    fn ulid(doc: &TodoLinkDoc) -> &str {
        &doc.ulid
    }

    fn rev(doc: &TodoLinkDoc) -> u64 {
        doc.rev
    }

    fn deleted(doc: &TodoLinkDoc) -> bool {
        doc.deleted
    }

    fn validate(doc: &TodoLinkDoc) -> Result<(), String> {
        doc.kind.parse::<LinkKind>()?;
        doc.target_kind.parse::<TargetKind>()?;
        Ok(())
    }

    fn bind_data<'q>(q: DataQuery<'q>, doc: &'q TodoLinkDoc) -> DataQuery<'q> {
        q.bind(&doc.from)
            .bind(&doc.kind)
            .bind(&doc.target_kind)
            .bind(&doc.target_ref)
    }

    /// Two offline devices can add the SAME connection (same from/kind/target)
    /// under different ulids — client-side dedupe can't see across devices.
    /// Land the newcomer already tombstoned when a live semantic twin exists:
    /// the earlier edge wins, and the duplicate dies on every device through
    /// the normal pull. No FOR UPDATE here: it would take the todo_links lock
    /// before next_rev's sync_rev lock, reversing the lock order the REST path
    /// uses (sync_rev first) and risking a deadlock. A rare race that slips two
    /// live twins through is caught by the boot-time dedupe_todo_links backstop.
    async fn tombstone_on_insert(
        tx: &mut MySqlConnection,
        user_id: &str,
        doc: &TodoLinkDoc,
    ) -> sqlx::Result<bool> {
        let twin: Option<(u64,)> = sqlx::query_as(
            "SELECT id FROM todo_links WHERE user_id = ? AND from_ulid = ? AND kind = ? \
             AND target_kind = ? AND target_ref = ? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(user_id)
        .bind(&doc.from)
        .bind(&doc.kind)
        .bind(&doc.target_kind)
        .bind(&doc.target_ref)
        .fetch_optional(&mut *tx)
        .await?;
        Ok(twin.is_some())
    }
}

pub async fn pull_todo_link(
    pool: &MySqlPool,
    user_id: &str,
    since: u64,
    limit: u64,
) -> Result<PullResponse<TodoLinkDoc>> {
    pull::<TodoLink>(pool, user_id, since, limit).await
}

pub async fn push_todo_link(
    pool: &MySqlPool,
    user_id: &str,
    entries: Vec<PushEntry<TodoLinkDoc>>,
) -> Result<Vec<TodoLinkDoc>, PushError> {
    push::<TodoLink>(pool, user_id, entries).await
}

/// One-time + boot-time cleanup: tombstone live duplicate edges (same
/// user/from/kind/target under different ulids — created before the push-time
/// twin guard existed, or by a rare race). The lowest id survives; each
/// tombstone gets its own rev so it propagates like any other delete.
/// Idempotent and cheap once clean.
pub async fn dedupe_todo_links(pool: &MySqlPool) -> Result<u64> {
    let dups: Vec<(u64,)> = sqlx::query_as(
        "SELECT t.id FROM todo_links t JOIN todo_links k \
         ON k.user_id = t.user_id AND k.from_ulid = t.from_ulid AND k.kind = t.kind \
         AND k.target_kind = t.target_kind AND k.target_ref = t.target_ref \
         AND k.deleted_at IS NULL AND k.id < t.id \
         WHERE t.deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await?;
    let mut n = 0u64;
    for (id,) in dups {
        let mut tx = pool.begin().await?;
        let rev = next_rev(&mut tx).await?;
        let res = sqlx::query(
            "UPDATE todo_links SET deleted_at = NOW(), rev = ?, updated_at = NOW() \
             WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(rev)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        n += res.rows_affected();
    }
    Ok(n)
}

// ---- wellbeing --------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct WellbeingDocRow {
    id: u64,
    ulid: String,
    recorded_at: NaiveDateTime,
    score: u8,
    energy: Option<u8>,
    /// JSON array of leaf words, as stored; parsed in `row_doc` (invalid →
    /// error — a corrupt row must fail the read, not pull as "no emotions").
    emotions: Option<String>,
    note: Option<String>,
    deleted: i64,
    rev: u64,
}

struct Wellbeing;

impl SyncSpec for Wellbeing {
    type Doc = WellbeingDoc;
    type Row = WellbeingDocRow;

    const TABLE: &'static str = "wellbeing";
    const DATA_COLS: &'static [&'static str] =
        &["recorded_at", "score", "energy", "emotions", "note"];

    fn row_rev(row: &WellbeingDocRow) -> u64 {
        row.rev
    }

    fn row_doc(r: WellbeingDocRow) -> Result<WellbeingDoc> {
        // The write path fails loudly on unserialisable emotions; the read must
        // match — serving a corrupt row as "no emotions" would propagate the
        // loss to every device invisibly.
        let emotions = match r.emotions.as_deref() {
            Some(s) => serde_json::from_str(s)
                .with_context(|| format!("wellbeing {}: corrupt emotions column", r.ulid))?,
            None => Vec::new(),
        };
        Ok(WellbeingDoc {
            ulid: r.ulid,
            id: Some(r.id),
            recorded_at: DateTime::from_naive_utc_and_offset(r.recorded_at, Utc),
            score: r.score,
            energy: r.energy,
            emotions,
            note: r.note,
            deleted: r.deleted != 0,
            rev: r.rev,
        })
    }

    fn ulid(doc: &WellbeingDoc) -> &str {
        &doc.ulid
    }

    fn rev(doc: &WellbeingDoc) -> u64 {
        doc.rev
    }

    fn deleted(doc: &WellbeingDoc) -> bool {
        doc.deleted
    }

    fn validate(doc: &WellbeingDoc) -> Result<(), String> {
        if !(1..=5).contains(&doc.score) {
            return Err(format!("wellbeing score {} out of range 1..=5", doc.score));
        }
        if let Some(e) = doc.energy
            && !(1..=5).contains(&e)
        {
            return Err(format!("wellbeing energy {e} out of range 1..=5"));
        }
        Ok(())
    }

    fn bind_data<'q>(q: DataQuery<'q>, doc: &'q WellbeingDoc) -> DataQuery<'q> {
        // A Vec<String> always serialises; .expect documents the invariant and
        // fails loudly rather than silently dropping the user's emotions.
        let emotions_json =
            serde_json::to_string(&doc.emotions).expect("Vec<String> serialises to JSON");
        q.bind(doc.recorded_at.naive_utc())
            .bind(doc.score)
            .bind(doc.energy)
            .bind(emotions_json)
            .bind(&doc.note)
    }
}

pub async fn pull_wellbeing(
    pool: &MySqlPool,
    user_id: &str,
    since: u64,
    limit: u64,
) -> Result<PullResponse<WellbeingDoc>> {
    pull::<Wellbeing>(pool, user_id, since, limit).await
}

pub async fn push_wellbeing(
    pool: &MySqlPool,
    user_id: &str,
    entries: Vec<PushEntry<WellbeingDoc>>,
) -> Result<Vec<WellbeingDoc>, PushError> {
    push::<Wellbeing>(pool, user_id, entries).await
}
