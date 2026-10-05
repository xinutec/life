//! Where sources disagree with the canonical row. Divergences are computed live;
//! a decision records the value set it settled, so it stays quiet until a
//! source's value changes.

use std::collections::{BTreeSet, HashMap};

use anyhow::{Result, anyhow};
use sqlx::types::Json;
use sqlx::{MySqlConnection, MySqlPool};

use crate::error::AppError;
use crate::products::ids::ProductId;
use crate::products::source::Source;
use crate::products::types::{
    Candidate, Choice, FieldChoice, FieldDivergence, Product, ReconcileField, Reconciler,
};

use super::facts::{fact_display, facts_by_source_in};
use super::{Listing, get_by_id, listings_for};

struct ReconciledField {
    field: ReconcileField,
    current: fn(&Product) -> Option<String>,
    offered: fn(&Listing) -> Option<String>,
    /// The UPDATE, bound `(value, source, product_id)`: a whole statement per
    /// field, so a column name is never spliced from a request.
    adopt_sql: &'static str,
}

/// The single-valued fields; the picture and facts reconcile their own way.
const RECONCILED_FIELDS: &[ReconciledField] = &[
    ReconciledField {
        field: ReconcileField::Name,
        adopt_sql: "UPDATE products SET name = ?, name_source = ? WHERE id = ?",
        current: |p| p.name.clone(),
        offered: |l| l.raw_name.clone(),
    },
    ReconciledField {
        field: ReconcileField::Brand,
        adopt_sql: "UPDATE products SET brand = ?, brand_source = ? WHERE id = ?",
        current: |p| p.brand.clone(),
        offered: |l| l.brand.clone(),
    },
    ReconciledField {
        field: ReconcileField::QuantityLabel,
        adopt_sql: "UPDATE products SET quantity_label = ?, quantity_label_source = ? WHERE id = ?",
        current: |p| p.quantity_label.clone(),
        offered: |l| l.quantity_label.clone(),
    },
];

fn trimmed(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// The canonical value and every listing's, sorted: a decision's suppression
/// key. Case-sensitive, so a source's different capitals stay a choice.
fn value_set(spec: &ReconciledField, product: &Product, listings: &[Listing]) -> Vec<String> {
    let mut set = BTreeSet::new();
    if let Some(v) = trimmed((spec.current)(product)) {
        set.insert(v);
    }
    for l in listings {
        if let Some(v) = trimmed((spec.offered)(l)) {
            set.insert(v);
        }
    }
    set.into_iter().collect()
}

/// field → the value set when it was decided.
pub type DecisionMap = HashMap<ReconcileField, Vec<String>>;

pub async fn field_decisions(pool: &MySqlPool, product_id: ProductId) -> Result<DecisionMap> {
    let rows: Vec<(ReconcileField, Json<Vec<String>>)> = sqlx::query_as(
        "SELECT field, seen_values FROM product_field_decisions WHERE product_id = ?",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;
    // An unknown field fails the query rather than reviving what it settled.
    Ok(rows.into_iter().map(|(f, v)| (f, v.0)).collect())
}

/// Unsettled disagreements. Pure; the caller fetches.
pub fn divergences(
    product: &Product,
    listings: &[Listing],
    decisions: &DecisionMap,
) -> Vec<FieldDivergence> {
    let mut out = Vec::new();
    for spec in RECONCILED_FIELDS {
        let current = trimmed((spec.current)(product));
        let candidates: Vec<Candidate> = listings
            .iter()
            .filter_map(|l| {
                let value = trimmed((spec.offered)(l))?;
                (current.as_deref() != Some(value.as_str())).then_some(Candidate {
                    source: l.source,
                    value,
                })
            })
            .collect();
        if candidates.is_empty() {
            continue;
        }
        let set = value_set(spec, product, listings);
        if decisions.get(&spec.field) == Some(&set) {
            continue;
        }
        out.push(FieldDivergence {
            field: spec.field,
            label: spec.field.label().to_string(),
            current,
            candidates,
        });
    }
    out
}

/// The picture's suppression key: its provenance and every offered URL.
fn picture_value_set(product: &Product, listings: &[Listing]) -> Vec<String> {
    let mut set = BTreeSet::new();
    // Marked so it cannot collide with a URL.
    set.insert(match product.image_source {
        Some(s) => format!("@{s}"),
        None => "@".to_string(),
    });
    for l in listings {
        if let Some(url) = trimmed(l.image_url.clone()) {
            set.insert(url);
        }
    }
    set.into_iter().collect()
}

/// A listing from a source other than our picture's is a candidate. Bytes and
/// URLs do not compare, so this goes by provenance; a hand upload is never
/// nagged. Pure.
pub fn picture_divergence(
    product: &Product,
    listings: &[Listing],
    decisions: &DecisionMap,
) -> Option<FieldDivergence> {
    if product.image_source == Some(Source::User) {
        return None;
    }
    let current_src = product.image_source;
    let candidates: Vec<Candidate> = listings
        .iter()
        .filter_map(|l| {
            let url = trimmed(l.image_url.clone())?;
            (Some(l.source) != current_src).then_some(Candidate {
                source: l.source,
                value: url,
            })
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let set = picture_value_set(product, listings);
    if decisions.get(&ReconcileField::Picture) == Some(&set) {
        return None;
    }
    Some(FieldDivergence {
        field: ReconcileField::Picture,
        label: ReconcileField::Picture.label().to_string(),
        current: current_src.map(|s| s.to_string()),
        candidates,
    })
}

/// Adopting needs bytes fetched over the network, so the route resolves this
/// before the transaction opens.
pub enum PictureChoice {
    Keep,
    Adopt {
        source: Source,
        bytes: Vec<u8>,
        mime: String,
    },
}

/// A choice the data cannot honour (400), or anything else (500).
#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    #[error("{0}")]
    Refused(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<ReconcileError> for AppError {
    fn from(e: ReconcileError) -> Self {
        match e {
            ReconcileError::Refused(msg) => AppError::BadRequest(msg),
            ReconcileError::Db(e) => AppError::Other(e.into()),
            ReconcileError::Other(e) => AppError::Other(e),
        }
    }
}

fn refuse<T>(msg: String) -> Result<T, ReconcileError> {
    Err(ReconcileError::Refused(msg))
}

/// Apply the choices and record each settled value set, in one transaction with
/// the product row locked: every choice applies, or none.
pub async fn reconcile(
    pool: &MySqlPool,
    product_id: ProductId,
    picture: Option<PictureChoice>,
    choices: &[FieldChoice],
) -> Result<(), ReconcileError> {
    let mut tx = pool.begin().await?;
    let held: Option<(Option<Source>,)> =
        sqlx::query_as("SELECT image_source FROM products WHERE id = ? FOR UPDATE")
            .bind(product_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((image_source,)) = held else {
        return Err(anyhow!("no such product: {product_id}").into());
    };
    let listings = listings_for(&mut *tx, product_id).await?;
    if let Some(picture) = picture {
        if let PictureChoice::Adopt {
            source,
            bytes,
            mime,
        } = picture
        {
            // An upload that landed after the page was drawn is not replaced.
            if image_source == Some(Source::User) {
                return refuse("the picture was uploaded by hand".into());
            }
            sqlx::query(
                "UPDATE products SET image = ?, image_mime = ?, image_source = ?, \
                 fetched_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(bytes)
            .bind(mime)
            .bind(source)
            .bind(product_id)
            .execute(&mut *tx)
            .await?;
        }
        // After any change, so the set holds the new provenance.
        let product = current(&mut tx, product_id).await?;
        let set = picture_value_set(&product, &listings);
        upsert_decision(&mut tx, product_id, ReconcileField::Picture, &set).await?;
    }
    for c in choices {
        reconcile_field(&mut tx, product_id, &listings, c).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn current(conn: &mut MySqlConnection, product_id: ProductId) -> Result<Product> {
    get_by_id(conn, product_id)
        .await?
        .ok_or_else(|| anyhow!("no such product: {product_id}"))
}

async fn reconcile_field(
    conn: &mut MySqlConnection,
    product_id: ProductId,
    listings: &[Listing],
    c: &FieldChoice,
) -> Result<(), ReconcileError> {
    let spec = match c.field.reconciler() {
        // These record which source to trust (0035); nothing is copied.
        Reconciler::Fact => return reconcile_fact(conn, product_id, c).await,
        Reconciler::Picture => return refuse("the picture is chosen apart from the fields".into()),
        Reconciler::Scalar => RECONCILED_FIELDS
            .iter()
            .find(|s| s.field == c.field)
            .ok_or_else(|| anyhow!("no reconcile spec for {}", c.field))?,
    };
    match c.choice {
        Choice::Keep => {}
        Choice::User => {
            let value = c.value.as_deref().map(str::trim).filter(|v| !v.is_empty());
            let Some(value) = value else {
                return refuse(format!("choosing our own {} needs a value", c.field));
            };
            set_canonical_field(conn, product_id, spec, value, Source::User).await?;
        }
        adopt => {
            let Some(source) = adopt.source() else {
                return refuse(format!("{adopt} is not a source"));
            };
            let value = listings
                .iter()
                .find(|l| l.source == source)
                .and_then(|l| trimmed((spec.offered)(l)));
            let Some(value) = value else {
                return refuse(format!("source {source} offers no {} to adopt", c.field));
            };
            set_canonical_field(conn, product_id, spec, &value, source).await?;
        }
    }
    // After applying, so the set is the settled one.
    let product = current(conn, product_id).await?;
    let set = value_set(spec, &product, listings);
    upsert_decision(conn, product_id, spec.field, &set).await?;
    Ok(())
}

/// Record which source to trust for a fact; `keep` records the current winner.
/// Never `user`: a nutrition panel is chosen among sources, not typed.
async fn reconcile_fact(
    conn: &mut MySqlConnection,
    product_id: ProductId,
    c: &FieldChoice,
) -> Result<(), ReconcileError> {
    if c.choice == Choice::User {
        return refuse(format!("{} is chosen by source, not typed", c.field));
    }
    let by_source = facts_by_source_in(conn, product_id).await?;
    let source = match c.choice.source() {
        // Precedence order, so the first that has it is the current pick.
        None => match by_source
            .iter()
            .find(|s| fact_display(c.field, &s.facts).is_some())
        {
            Some(s) => s.source,
            None => return refuse(format!("no source offers {} to keep", c.field)),
        },
        Some(want) => {
            let has = by_source
                .iter()
                .find(|s| s.source == want)
                .and_then(|s| fact_display(c.field, &s.facts))
                .is_some();
            if !has {
                return refuse(format!("source {want} offers no {} to adopt", c.field));
            }
            want
        }
    };
    sqlx::query(
        "INSERT INTO product_fact_sources (product_id, kind, source) \
         VALUES (?, ?, ?) \
         ON DUPLICATE KEY UPDATE source = VALUES(source), decided_at = CURRENT_TIMESTAMP",
    )
    .bind(product_id)
    .bind(c.field)
    .bind(source)
    .execute(conn)
    .await?;
    Ok(())
}

/// Takes the spec, not a name: only a field with an `adopt_sql` has one.
async fn set_canonical_field(
    conn: &mut MySqlConnection,
    product_id: ProductId,
    spec: &ReconciledField,
    value: &str,
    source: Source,
) -> Result<()> {
    // `*_source` records the adopted source, or `user`, which later refreshes
    // respect.
    //
    // dev-lint: allow-sqlx static literal chosen from RECONCILED_FIELDS, above
    sqlx::query(spec.adopt_sql)
        .bind(value)
        .bind(source)
        .bind(product_id)
        .execute(conn)
        .await?;
    Ok(())
}

async fn upsert_decision(
    conn: &mut MySqlConnection,
    product_id: ProductId,
    field: ReconcileField,
    set: &[String],
) -> Result<()> {
    let json = serde_json::to_string(set)?;
    sqlx::query(
        "INSERT INTO product_field_decisions (product_id, field, seen_values) \
         VALUES (?, ?, ?) \
         ON DUPLICATE KEY UPDATE seen_values = VALUES(seen_values), decided_at = CURRENT_TIMESTAMP",
    )
    .bind(product_id)
    .bind(field)
    .bind(json)
    .execute(conn)
    .await?;
    Ok(())
}
