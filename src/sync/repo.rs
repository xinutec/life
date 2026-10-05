//! The sync protocol, written once over `SyncSpec`, so its safety rules (row lock,
//! rev guard, set-only tombstone, commit-ordered revs, validation before writing)
//! cannot drift between collections (docs/design/sync.md).

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use sqlx::mysql::MySqlArguments;
use sqlx::query::Query;
use sqlx::{AssertSqlSafe, MySql, MySqlConnection, MySqlPool};
use ulid::Ulid;

use crate::error::AppError;
use crate::inventory::types::ItemCategory;
use crate::products::ids::ProductId;
use crate::todo::types::{LinkKind, TargetKind, TodoPriority, TodoStatus, TodoType};

use super::types::{
    Checkpoint, PullResponse, PushEntry, ShoppingDoc, TodoDoc, TodoLinkDoc, WellbeingDoc,
};

/// The next revision, inside the caller's transaction: the counter's row lock is
/// held until commit, so revisions follow commit order and a pull never passes an
/// uncommitted one. Take it before any row lock, so every sync write locks in one
/// order and none can deadlock another.
pub async fn next_rev(conn: &mut MySqlConnection) -> sqlx::Result<u64> {
    let res = sqlx::query("UPDATE sync_rev SET val = LAST_INSERT_ID(val + 1) WHERE id = 1")
        .execute(&mut *conn)
        .await?;
    Ok(res.last_insert_id())
}

/// One tombstone or restore with a fresh `rev`, in its own transaction. False
/// when no row matched.
pub async fn stamp<'q>(
    pool: &MySqlPool,
    query: impl FnOnce(u64) -> sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let rev = next_rev(&mut tx).await?;
    let res = query(rev).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(res.rows_affected() > 0)
}

/// Invalid input (400) apart from everything else (500). Invalid docs are refused,
/// never clamped or stored.
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

/// What is collection-specific: table, data columns in bind order, mapping and
/// validation.
trait SyncSpec {
    type Doc: Send;
    type Row: for<'r> sqlx::FromRow<'r, sqlx::mysql::MySqlRow> + Send + Unpin;

    const TABLE: &'static str;
    /// Beyond id, ulid, deleted and rev, in `bind_data`'s order.
    const DATA_COLS: &'static [&'static str];

    fn row_rev(row: &Self::Row) -> u64;
    fn row_doc(row: Self::Row) -> Result<Self::Doc>;
    fn ulid(doc: &Self::Doc) -> &str;
    fn rev(doc: &Self::Doc) -> u64;
    fn deleted(doc: &Self::Doc) -> bool;

    /// What the types cannot rule out (an out-of-range score); refused, never
    /// clamped.
    fn validate(_doc: &Self::Doc) -> Result<(), String> {
        Ok(())
    }

    fn bind_data<'q>(q: DataQuery<'q>, doc: &'q Self::Doc) -> DataQuery<'q>;

    /// Land a new row already tombstoned (the to-do-link twin dedupe).
    async fn tombstone_on_insert(
        _tx: &mut MySqlConnection,
        _user_id: &str,
        _doc: &Self::Doc,
    ) -> sqlx::Result<bool> {
        Ok(false)
    }
}

fn select_list<C: SyncSpec>() -> String {
    // A boolean SQL expression decodes as an integer, hence the CAST alias.
    format!(
        "id, ulid, {}, CAST(deleted_at IS NOT NULL AS SIGNED) AS deleted, rev",
        C::DATA_COLS.join(", ")
    )
}

/// Documents past the checkpoint, tombstones included, in rev order.
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
    // Only compile-time names are spliced; every value is bound.
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

/// An idempotent upsert per change, guarded by the client's assumed revision.
/// Returns the server's doc for each stale change; resolving is the client's job.
async fn push<C: SyncSpec>(
    pool: &MySqlPool,
    user_id: &str,
    entries: Vec<PushEntry<C::Doc>>,
) -> Result<Vec<C::Doc>, PushError> {
    // The whole batch first: each entry commits alone, so a late rejection would
    // leave the push half-applied.
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
        // A conflict rolls back, and the revision with it.
        let rev = next_rev(&mut tx).await?;
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
            // Set-only tombstones: no push clears one, so a stale client cannot
            // resurrect a delete. The trash restore is the one way back.
            C::bind_data(sqlx::query(AssertSqlSafe(update_sql.as_str())), &new)
                .bind(C::deleted(&new))
                .bind(rev)
                .bind(C::ulid(&new))
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
        } else {
            let tombstoned = C::tombstone_on_insert(&mut tx, user_id, &new).await?;
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

#[derive(sqlx::FromRow)]
struct ShoppingDocRow {
    id: u64,
    ulid: String,
    name: String,
    quantity: Option<f64>,
    unit: Option<String>,
    barcode: Option<String>,
    category: ItemCategory,
    product_id: Option<ProductId>,
    done: bool,
    deleted: i64,
    rev: u64,
}

struct Shopping;

impl SyncSpec for Shopping {
    type Doc = ShoppingDoc;
    type Row = ShoppingDocRow;

    const TABLE: &'static str = "shopping_items";
    const DATA_COLS: &'static [&'static str] = &[
        "name",
        "quantity",
        "unit",
        "barcode",
        "category",
        "product_id",
        "done",
    ];

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
            category: r.category,
            product_id: r.product_id,
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

    fn bind_data<'q>(q: DataQuery<'q>, doc: &'q ShoppingDoc) -> DataQuery<'q> {
        q.bind(&doc.name)
            .bind(doc.quantity)
            .bind(&doc.unit)
            .bind(&doc.barcode)
            .bind(doc.category)
            .bind(doc.product_id)
            .bind(doc.done)
    }
}

/// Gives pre-sync rows a ULID and revision. Idempotent.
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

#[derive(sqlx::FromRow)]
struct TodoDocRow {
    id: u64,
    ulid: String,
    title: String,
    todo_type: TodoType,
    status: TodoStatus,
    priority: Option<TodoPriority>,
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

    fn bind_data<'q>(q: DataQuery<'q>, doc: &'q TodoDoc) -> DataQuery<'q> {
        q.bind(&doc.title)
            .bind(doc.todo_type)
            .bind(doc.status)
            .bind(doc.priority)
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

#[derive(sqlx::FromRow)]
struct TodoLinkDocRow {
    id: u64,
    ulid: String,
    from_ulid: String,
    kind: LinkKind,
    target_kind: TargetKind,
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

    fn bind_data<'q>(q: DataQuery<'q>, doc: &'q TodoLinkDoc) -> DataQuery<'q> {
        q.bind(&doc.from)
            .bind(doc.kind)
            .bind(doc.target_kind)
            .bind(&doc.target_ref)
    }

    /// Two devices can add the same connection under different ulids: the newer
    /// lands tombstoned. The counter, already held, serialises this check against
    /// every other sync write.
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
        .bind(doc.kind)
        .bind(doc.target_kind)
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

/// Tombstone duplicate edges a race let through; the lowest id survives, so every
/// device agrees. Idempotent.
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
        let removed = stamp(pool, |rev| {
            sqlx::query(
                "UPDATE todo_links SET deleted_at = NOW(), rev = ?, updated_at = NOW() \
                 WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(rev)
            .bind(id)
        })
        .await?;
        n += u64::from(removed);
    }
    Ok(n)
}

#[derive(sqlx::FromRow)]
struct WellbeingDocRow {
    id: u64,
    ulid: String,
    recorded_at: NaiveDateTime,
    score_tenths: u8,
    energy_tenths: Option<u8>,
    /// Parsed in `row_doc`; a corrupt row fails the read rather than pull as none.
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
    const DATA_COLS: &'static [&'static str] = &[
        "recorded_at",
        "score_tenths",
        "energy_tenths",
        "emotions",
        "note",
    ];

    fn row_rev(row: &WellbeingDocRow) -> u64 {
        row.rev
    }

    fn row_doc(r: WellbeingDocRow) -> Result<WellbeingDoc> {
        let emotions = match r.emotions.as_deref() {
            Some(s) => serde_json::from_str(s)
                .with_context(|| format!("wellbeing {}: corrupt emotions column", r.ulid))?,
            None => Vec::new(),
        };
        Ok(WellbeingDoc {
            ulid: r.ulid,
            id: Some(r.id),
            recorded_at: DateTime::from_naive_utc_and_offset(r.recorded_at, Utc),
            score_tenths: r.score_tenths,
            energy_tenths: r.energy_tenths,
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
        check_tenths("score", doc.score_tenths)?;
        if let Some(e) = doc.energy_tenths {
            check_tenths("energy", e)?;
        }
        Ok(())
    }

    fn bind_data<'q>(q: DataQuery<'q>, doc: &'q WellbeingDoc) -> DataQuery<'q> {
        // Always serialises.
        let emotions_json =
            serde_json::to_string(&doc.emotions).expect("Vec<String> serialises to JSON");
        q.bind(doc.recorded_at.naive_utc())
            .bind(doc.score_tenths)
            .bind(doc.energy_tenths)
            .bind(emotions_json)
            .bind(&doc.note)
    }
}

/// Half-points in tenths: 10, 15 … 50. A 37 is refused, not rounded: a rounded
/// reading is one the user never gave.
const STEP_TENTHS: u8 = 5;

fn check_tenths(what: &str, tenths: u8) -> Result<(), String> {
    if !(10..=50).contains(&tenths) {
        return Err(format!(
            "wellbeing {what} {tenths} tenths out of range 10..=50 (1.0..=5.0)"
        ));
    }
    if !tenths.is_multiple_of(STEP_TENTHS) {
        return Err(format!(
            "wellbeing {what} {tenths} tenths is not a half-point step"
        ));
    }
    Ok(())
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
