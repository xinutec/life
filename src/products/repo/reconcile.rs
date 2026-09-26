//! Reconciliation: where sources disagree with the canonical row.
//!
//! Each source's account lives on its listing; the canonical `products` row
//! holds one value per field. A disagreement not yet settled is a divergence
//! to approve. Divergences are computed live; a decision records the value set
//! it settled, so it stays quiet until a source's value changes.

use std::collections::{BTreeSet, HashMap};

use anyhow::Result;
use sqlx::MySqlPool;
use sqlx::types::Json;

use crate::products::ids::ProductId;
use crate::products::source::Source;
use crate::products::types::{
    Candidate, Choice, FieldChoice, FieldDivergence, Product, ReconcileField, Reconciler,
};

use super::facts::{fact_display, facts_by_source};
use super::{Listing, get_by_id, listings_for};

/// A canonical scalar field reconciliation covers: how to read its current value
/// off the product and its offered value off a listing.
struct ReconciledField {
    field: ReconcileField,
    current: fn(&Product) -> Option<String>,
    offered: fn(&Listing) -> Option<String>,
    /// The UPDATE that adopts a value for this field, bound `(value, source,
    /// product_id)`. It lives in the table so `set_canonical_field` picks a
    /// whole statement rather than splicing a column name — the column can
    /// never come from the request, and there is no "unknown field" arm to fall
    /// through, because a non-scalar field cannot reach it.
    adopt_sql: &'static str,
}

/// The fields with a single canonical value that a source can disagree about.
/// (Picture and the facts reconcile through their own mechanisms, below.)
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

/// The distinct values on the table for a field — the canonical value plus every
/// listing's — sorted. Stored with a decision as its suppression key: while this
/// set is unchanged the divergence stays settled; any change re-surfaces it.
///
/// Case-sensitive on purpose: a source that differs only in capitals offers a
/// spelling to adopt or refuse, which folding case would hide.
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

/// field → the value set that was on the table when it was last decided.
pub type DecisionMap = HashMap<ReconcileField, Vec<String>>;

/// The decisions settled for a product's fields.
pub async fn field_decisions(pool: &MySqlPool, product_id: ProductId) -> Result<DecisionMap> {
    let rows: Vec<(String, Json<Vec<String>>)> = sqlx::query_as(
        "SELECT field, seen_values FROM product_field_decisions WHERE product_id = ?",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;
    // A field we no longer know about is a hard error, not a row to skip: it
    // would mean a decision is being silently ignored, and the divergence it
    // settled would quietly come back.
    rows.into_iter()
        .map(|(f, v)| {
            Ok((
                f.parse::<ReconcileField>().map_err(anyhow::Error::msg)?,
                v.0,
            ))
        })
        .collect()
}

/// Where the sources disagree with the canonical row and it isn't already
/// settled. Pure — the I/O (listings, decisions) is fetched by the caller, so
/// the rule is unit-testable without a database.
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
        // Settled if the exact value set on the table matches what was decided.
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

/// The suppression key for a picture decision: the current image's provenance
/// plus every offered picture URL. Any change (a new source picture, or the
/// canonical picture's source changing) alters the set and re-surfaces the
/// divergence; while it's unchanged the decision keeps it settled.
fn picture_value_set(product: &Product, listings: &[Listing]) -> Vec<String> {
    let mut set = BTreeSet::new();
    // The current holder, marked so it can't collide with a URL and so adopting a
    // different source (which changes provenance) re-keys the decision.
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

/// The picture disagreement, if any. The canonical image is bytes and a listing
/// offers a URL, so there's nothing to value-compare; instead a listing from a
/// source OTHER than the one our picture came from is a candidate — "this shop
/// has its own picture you could adopt". A hand-uploaded picture (`image_source`
/// == `user`) is ours and never nagged. Pure — I/O is the caller's.
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
        // The source we currently hold a picture from (if any) — the frontend
        // shows the actual thumbnail; this is the provenance behind it.
        current: product
            .has_image
            .then(|| current_src.map(|s| s.to_string()).unwrap_or_default()),
        candidates,
    })
}

/// Record the picture bytes' provenance (which source it came from, or `user`).
pub async fn set_image_provenance(
    pool: &MySqlPool,
    product_id: ProductId,
    source: Source,
) -> Result<()> {
    sqlx::query("UPDATE products SET image_source = ? WHERE id = ?")
        .bind(source)
        .bind(product_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Settle the picture divergence: record the value set as it stands NOW (after any
/// adoption has changed the bytes' provenance), so it stays quiet until a source's
/// picture — or ours — changes. Call after applying the picture choice.
pub async fn settle_picture(pool: &MySqlPool, product_id: ProductId) -> Result<()> {
    let product = get_by_id(pool, product_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no such product: {product_id}"))?;
    let listings = listings_for(pool, product_id).await?;
    let set = picture_value_set(&product, &listings);
    upsert_decision(pool, product_id, ReconcileField::Picture, &set).await
}

/// Apply reconcile decisions: set the canonical row from the chosen source, our
/// own typed value, or leave it (keep); then record the settled value set so the
/// divergence stays quiet until a source's value changes.
pub async fn reconcile(
    pool: &MySqlPool,
    product_id: ProductId,
    choices: &[FieldChoice],
) -> Result<()> {
    let listings = listings_for(pool, product_id).await?;
    for c in choices {
        match c.field.reconciler() {
            // Nutrition and ingredients settle by recording which source to
            // trust (0035), not by writing the canonical row.
            Reconciler::Fact => {
                reconcile_fact(pool, product_id, c).await?;
                continue;
            }
            // The route adopts the picture itself (its bytes come through the
            // SSRF gate) and calls `settle_picture`; it never reaches here.
            Reconciler::Picture => anyhow::bail!("the picture is settled by the route"),
            Reconciler::Scalar => {}
        }
        let Some(spec) = RECONCILED_FIELDS.iter().find(|s| s.field == c.field) else {
            // Unreachable while every Scalar field has a row above; a missing
            // one is a programming error, not bad input.
            anyhow::bail!("no reconcile spec for {}", c.field);
        };
        match c.choice {
            Choice::Keep => {}
            Choice::User => {
                // Our own value: taken from the request, not a listing.
                let value = c.value.as_deref().map(str::trim).filter(|v| !v.is_empty());
                let Some(value) = value else {
                    anyhow::bail!("choosing our own {} needs a value", c.field);
                };
                set_canonical_field(pool, product_id, spec, value, Source::User).await?;
            }
            adopt => {
                let source = adopt
                    .source()
                    .ok_or_else(|| anyhow::anyhow!("{adopt} is not a source"))?;
                let value = listings
                    .iter()
                    .find(|l| l.source == source)
                    .and_then(|l| trimmed((spec.offered)(l)));
                let Some(value) = value else {
                    anyhow::bail!("source {source} offers no {} to adopt", c.field);
                };
                set_canonical_field(pool, product_id, spec, &value, source).await?;
            }
        }
        // Recompute the set AFTER applying so the decision reflects the settled
        // state (the adopted value is now the canonical one).
        let product = get_by_id(pool, product_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no such product: {product_id}"))?;
        let set = value_set(spec, &product, &listings);
        upsert_decision(pool, product_id, spec.field, &set).await?;
    }
    Ok(())
}

/// Settle a picked fact (nutrition / ingredients): record which source to trust.
/// `KEEP` records the current precedence winner (so the divergence stays quiet);
/// a source id records that source, if it actually offers the fact. `USER` is
/// rejected — these facts are chosen among sources, never typed by hand (unlike
/// the scalar fields), so we never invent a nutrition panel or ingredient list.
async fn reconcile_fact(pool: &MySqlPool, product_id: ProductId, c: &FieldChoice) -> Result<()> {
    if c.choice == Choice::User {
        anyhow::bail!("{} is chosen by source, not typed", c.field);
    }
    let by_source = facts_by_source(pool, product_id).await?;
    let source = match c.choice.source() {
        // by_source is precedence-ordered; the first source that has this fact
        // is the current pick.
        None => by_source
            .iter()
            .find(|s| fact_display(c.field, &s.facts).is_some())
            .map(|s| s.source)
            .ok_or_else(|| anyhow::anyhow!("no source offers {} to keep", c.field))?,
        Some(want) => {
            let has = by_source
                .iter()
                .find(|s| s.source == want)
                .and_then(|s| fact_display(c.field, &s.facts))
                .is_some();
            if !has {
                anyhow::bail!("source {want} offers no {} to adopt", c.field);
            }
            want
        }
    };
    upsert_fact_source(pool, product_id, c.field, source).await
}

/// Record (or change) the source picked to trust for a fact kind (0035).
async fn upsert_fact_source(
    pool: &MySqlPool,
    product_id: ProductId,
    kind: ReconcileField,
    source: Source,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO product_fact_sources (product_id, kind, source) \
         VALUES (?, ?, ?) \
         ON DUPLICATE KEY UPDATE source = VALUES(source), decided_at = CURRENT_TIMESTAMP",
    )
    .bind(product_id)
    .bind(kind.as_str())
    .bind(source)
    .execute(pool)
    .await?;
    Ok(())
}

/// Set one canonical scalar field to an adopted value.
///
/// Takes the spec rather than a field name: a `ReconciledField` only exists for
/// a field that has an `adopt_sql`, so "which column" is settled by construction
/// and there is nothing to reject at runtime.
async fn set_canonical_field(
    pool: &MySqlPool,
    product_id: ProductId,
    spec: &ReconciledField,
    value: &str,
    source: Source,
) -> Result<()> {
    // Each reconcilable scalar carries a provenance column (`*_source`): the
    // adopted source, or `user` for our own correction. `user` there is what a
    // later source refresh checks before touching the value.
    //
    // dev-lint: allow-sqlx static literal chosen from RECONCILED_FIELDS, above
    sqlx::query(spec.adopt_sql)
        .bind(value)
        .bind(source)
        .bind(product_id)
        .execute(pool)
        .await?;
    Ok(())
}

async fn upsert_decision(
    pool: &MySqlPool,
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
    .bind(field.as_str())
    .bind(json)
    .execute(pool)
    .await?;
    Ok(())
}
