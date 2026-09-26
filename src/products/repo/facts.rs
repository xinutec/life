//! Product facts (nutrition, ingredients, allergens, dietary flags) and the
//! source documents they were read from.
//!
//! Stored per source, so OFF and a retailer's Brandbank facts sit side by side;
//! a write restates one source, and `facts_for` merges them on read
//! (precedence for nutrition and ingredients, union for allergens).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::Result;
use sqlx::types::Json;
use sqlx::{MySqlConnection, MySqlPool};

use crate::products::ids::ProductId;
use crate::products::nutrition::{
    Allergen, Claim, DietaryFlag, Nutrition, Presence, ProductFacts, fact_rank, merge_allergens,
    merge_dietary, merge_ingredients, merge_nutrition, summarize_nutrition,
};
use crate::products::source::Source;
use crate::products::types::{
    Candidate, FieldDivergence, ReconcileField, Reconciler, SourceDocument, SourceFacts,
};

#[derive(sqlx::FromRow)]
struct NutritionRow {
    source: Source,
    basis: String,
    serving_size: Option<String>,
    energy_kj: Option<f64>,
    energy_kcal: Option<f64>,
    fat_g: Option<f64>,
    saturates_g: Option<f64>,
    carbohydrate_g: Option<f64>,
    sugars_g: Option<f64>,
    fibre_g: Option<f64>,
    protein_g: Option<f64>,
    salt_g: Option<f64>,
    extra: Option<Json<BTreeMap<String, f64>>>,
}

/// Upsert this source's nutrition panel for a product (keyed by product + source
/// since 0033, so OFF's and a retailer's panels coexist).
pub async fn upsert_nutrition(
    pool: &MySqlPool,
    product_id: ProductId,
    n: &Nutrition,
    source: Source,
) -> Result<()> {
    let mut conn = pool.acquire().await?;
    upsert_nutrition_in(&mut conn, product_id, n, source).await
}

async fn upsert_nutrition_in(
    conn: &mut MySqlConnection,
    product_id: ProductId,
    n: &Nutrition,
    source: Source,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO product_nutrition \
         (product_id, basis, serving_size, energy_kj, energy_kcal, fat_g, saturates_g, \
          carbohydrate_g, sugars_g, fibre_g, protein_g, salt_g, extra, source) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE basis = VALUES(basis), serving_size = VALUES(serving_size), \
          energy_kj = VALUES(energy_kj), energy_kcal = VALUES(energy_kcal), fat_g = VALUES(fat_g), \
          saturates_g = VALUES(saturates_g), carbohydrate_g = VALUES(carbohydrate_g), \
          sugars_g = VALUES(sugars_g), fibre_g = VALUES(fibre_g), protein_g = VALUES(protein_g), \
          salt_g = VALUES(salt_g), extra = VALUES(extra)",
    )
    .bind(product_id)
    .bind(&n.basis)
    .bind(&n.serving_size)
    .bind(n.energy_kj)
    .bind(n.energy_kcal)
    .bind(n.fat_g)
    .bind(n.saturates_g)
    .bind(n.carbohydrate_g)
    .bind(n.sugars_g)
    .bind(n.fibre_g)
    .bind(n.protein_g)
    .bind(n.salt_g)
    .bind(Json(&n.extra))
    .bind(source)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Set the product's pack-size label (e.g. Asda's "22x27G").
pub async fn set_quantity_label(
    pool: &MySqlPool,
    product_id: ProductId,
    label: &str,
) -> Result<()> {
    sqlx::query("UPDATE products SET quantity_label = ? WHERE id = ?")
        .bind(label)
        .bind(product_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Upsert this source's ingredients text (one block per product+source, 0033).
pub async fn set_ingredients(
    pool: &MySqlPool,
    product_id: ProductId,
    text: &str,
    source: Source,
) -> Result<()> {
    let mut conn = pool.acquire().await?;
    set_ingredients_in(&mut conn, product_id, text, source).await
}

async fn set_ingredients_in(
    conn: &mut MySqlConnection,
    product_id: ProductId,
    text: &str,
    source: Source,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO product_ingredients (product_id, source, text) VALUES (?, ?, ?) \
         ON DUPLICATE KEY UPDATE text = VALUES(text)",
    )
    .bind(product_id)
    .bind(source)
    .bind(text)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Replace this source's allergen set (empty clears it), leaving other sources'
/// rows alone; `facts_for` unions them on read.
///
/// One transaction: a half-applied replace would read as the product not
/// containing an allergen, a health error rather than a data-quality one.
pub async fn replace_allergens(
    pool: &MySqlPool,
    product_id: ProductId,
    allergens: &[Allergen],
    source: Source,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    replace_allergens_in(&mut tx, product_id, allergens, source).await?;
    tx.commit().await?;
    Ok(())
}

async fn replace_allergens_in(
    conn: &mut MySqlConnection,
    product_id: ProductId,
    allergens: &[Allergen],
    source: Source,
) -> Result<()> {
    sqlx::query("DELETE FROM product_allergens WHERE product_id = ? AND source = ?")
        .bind(product_id)
        .bind(source)
        .execute(&mut *conn)
        .await?;
    for a in allergens {
        sqlx::query(
            "INSERT INTO product_allergens (product_id, allergen, presence, source) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(product_id)
        .bind(&a.allergen)
        .bind(a.presence.to_string())
        .bind(source)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Replace THIS SOURCE's dietary flags, leaving other sources' claims alone
/// (migration 0028). Open Food Facts and Asda each tag the same product, and a
/// re-lookup of one must not erase the other's contribution; `facts_for` merges
/// them on read.
///
/// Atomic for the same reason as `replace_allergens`: half a replace would drop
/// claims this source actually makes, and `merge_dietary` reads a missing "no"
/// as an unopposed "yes".
pub async fn replace_dietary(
    pool: &MySqlPool,
    product_id: ProductId,
    flags: &[DietaryFlag],
    source: Source,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    replace_dietary_in(&mut tx, product_id, flags, source).await?;
    tx.commit().await?;
    Ok(())
}

async fn replace_dietary_in(
    conn: &mut MySqlConnection,
    product_id: ProductId,
    flags: &[DietaryFlag],
    source: Source,
) -> Result<()> {
    sqlx::query("DELETE FROM product_dietary_flags WHERE product_id = ? AND source = ?")
        .bind(product_id)
        .bind(source)
        .execute(&mut *conn)
        .await?;
    for f in flags {
        sqlx::query(
            "INSERT INTO product_dietary_flags (product_id, flag, value, source) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(product_id)
        .bind(&f.flag)
        .bind(f.value.to_string())
        .bind(source)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Keep a fetched source payload verbatim (product_documents, 0034), so we never
/// fetch it twice and can re-derive from it later. Keyed by (product, source,
/// kind); re-fetching the same kind overwrites and re-stamps `fetched_at`.
pub async fn upsert_document(
    pool: &MySqlPool,
    product_id: ProductId,
    source: Source,
    kind: &str,
    body: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO product_documents (product_id, source, kind, body) VALUES (?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE body = VALUES(body), fetched_at = CURRENT_TIMESTAMP",
    )
    .bind(product_id)
    .bind(source)
    .bind(kind)
    .bind(body)
    .execute(pool)
    .await?;
    Ok(())
}

/// The raw payload we hold for (product, source, kind), if any — for re-parsing
/// without another fetch.
pub async fn get_document(
    pool: &MySqlPool,
    product_id: ProductId,
    source: Source,
    kind: &str,
) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT body FROM product_documents WHERE product_id = ? AND source = ? AND kind = ?",
    )
    .bind(product_id)
    .bind(source)
    .bind(kind)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(b,)| b))
}

#[derive(sqlx::FromRow)]
struct DocRow {
    source: Source,
    kind: String,
    fetched_at: i64,
    bytes: i64,
}

/// Metadata for every raw payload held for a product (not the bodies) — what the
/// product detail advertises so the client needn't re-fetch what we already have.
pub async fn documents_for(pool: &MySqlPool, product_id: ProductId) -> Result<Vec<SourceDocument>> {
    let rows: Vec<DocRow> = sqlx::query_as(
        "SELECT source, kind, \
         CAST(UNIX_TIMESTAMP(fetched_at) * 1000 AS SIGNED) AS fetched_at, \
         CAST(LENGTH(body) AS SIGNED) AS bytes \
         FROM product_documents WHERE product_id = ? ORDER BY source, kind",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| SourceDocument {
            source: r.source,
            kind: r.kind,
            fetched_at: r.fetched_at,
            bytes: r.bytes,
        })
        .collect())
}

/// Persist a product's full fact set from one source, each part restated. Skips
/// nutrition/ingredients the source didn't provide (leaving any existing rows);
/// allergens and dietary flags always replace (their absence is meaningful).
///
/// One transaction for the lot: a source's facts describe one product as that
/// source understands it, and a partly-stored set is a description nobody wrote —
/// Asda's nutrition beside OFF's allergens, attributed to Asda. Either the whole
/// account lands or none of it does.
pub async fn store_facts(
    pool: &MySqlPool,
    product_id: ProductId,
    facts: &ProductFacts,
    source: Source,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    if let Some(n) = &facts.nutrition {
        upsert_nutrition_in(&mut tx, product_id, n, source).await?;
    }
    if let Some(ing) = &facts.ingredients {
        set_ingredients_in(&mut tx, product_id, ing, source).await?;
    }
    replace_allergens_in(&mut tx, product_id, &facts.allergens, source).await?;
    replace_dietary_in(&mut tx, product_id, &facts.dietary, source).await?;
    tx.commit().await?;
    Ok(())
}

/// Read back everything we know about a product beyond its identity. Feeds
/// the product detail (GET /api/products/id/{id}) — the rich product page.
pub async fn facts_for(pool: &MySqlPool, product_id: ProductId) -> Result<ProductFacts> {
    let by_source = facts_by_source(pool, product_id).await?;
    let prefs = fact_source_prefs(pool, product_id).await?;
    Ok(merge_facts(&by_source, &prefs))
}

/// Every source's own account of a product's facts, one `SourceFacts` per source
/// that has any. This is the raw material both for the merged display
/// (`merge_facts`) and for provenance/divergence — fetched once, reasoned over
/// purely. Sources are returned in precedence order (retailer before crowd).
pub async fn facts_by_source(pool: &MySqlPool, product_id: ProductId) -> Result<Vec<SourceFacts>> {
    let nrows: Vec<NutritionRow> = sqlx::query_as(
        "SELECT source, basis, serving_size, energy_kj, energy_kcal, fat_g, saturates_g, \
         carbohydrate_g, sugars_g, fibre_g, protein_g, salt_g, extra \
         FROM product_nutrition WHERE product_id = ?",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;
    let ing_rows: Vec<(Source, String)> =
        sqlx::query_as("SELECT source, text FROM product_ingredients WHERE product_id = ?")
            .bind(product_id)
            .fetch_all(pool)
            .await?;
    let allergen_rows: Vec<(Source, String, String)> = sqlx::query_as(
        "SELECT source, allergen, presence FROM product_allergens WHERE product_id = ? \
         ORDER BY allergen",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;
    let dietary_rows: Vec<(Source, String, String)> = sqlx::query_as(
        "SELECT source, flag, value FROM product_dietary_flags WHERE product_id = ? \
         ORDER BY flag",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;

    // Group every table's rows by source into one ProductFacts each.
    let mut by_source: BTreeMap<Source, ProductFacts> = BTreeMap::new();
    let blank = || ProductFacts {
        nutrition: None,
        ingredients: None,
        allergens: Vec::new(),
        dietary: Vec::new(),
    };
    for r in nrows {
        by_source.entry(r.source).or_insert_with(blank).nutrition = Some(Nutrition {
            basis: r.basis,
            serving_size: r.serving_size,
            energy_kj: r.energy_kj,
            energy_kcal: r.energy_kcal,
            fat_g: r.fat_g,
            saturates_g: r.saturates_g,
            carbohydrate_g: r.carbohydrate_g,
            sugars_g: r.sugars_g,
            fibre_g: r.fibre_g,
            protein_g: r.protein_g,
            salt_g: r.salt_g,
            extra: r.extra.map(|j| j.0).unwrap_or_default(),
        });
    }
    for (source, text) in ing_rows {
        by_source.entry(source).or_insert_with(blank).ingredients = Some(text);
    }
    // The columns are ENUMs, so a value outside the taxonomy means the schema and
    // this code have diverged — a parse failure here is the honest report of that,
    // not something to paper over with a default.
    for (source, allergen, presence) in allergen_rows {
        let presence = presence
            .parse::<Presence>()
            .map_err(|e| anyhow::anyhow!("{e} (product {product_id}, source {source})"))?;
        by_source
            .entry(source)
            .or_insert_with(blank)
            .allergens
            .push(Allergen { allergen, presence });
    }
    for (source, flag, value) in dietary_rows {
        let value = value
            .parse::<Claim>()
            .map_err(|e| anyhow::anyhow!("{e} (product {product_id}, source {source})"))?;
        by_source
            .entry(source)
            .or_insert_with(blank)
            .dietary
            .push(DietaryFlag { flag, value });
    }

    let mut out: Vec<SourceFacts> = by_source
        .into_iter()
        .map(|(source, facts)| SourceFacts { source, facts })
        .collect();
    // Precedence order, so the UI lists the trusted source first.
    out.sort_by_key(|s| (fact_rank(s.source), s.source));
    Ok(out)
}

/// Which source to trust for each source-picked fact (0035).
pub type FactSourceMap = HashMap<ReconcileField, Source>;

/// The fact-source picks recorded for a product (empty if none — precedence then
/// decides the merge).
pub async fn fact_source_prefs(pool: &MySqlPool, product_id: ProductId) -> Result<FactSourceMap> {
    let rows: Vec<(String, Source)> =
        sqlx::query_as("SELECT kind, source FROM product_fact_sources WHERE product_id = ?")
            .bind(product_id)
            .fetch_all(pool)
            .await?;
    // As in `field_decisions`: a kind we no longer know is a hard error, because
    // silently dropping it would un-settle a fact you already decided.
    rows.into_iter()
        .map(|(k, src)| {
            Ok((
                k.parse::<ReconcileField>().map_err(anyhow::Error::msg)?,
                src,
            ))
        })
        .collect()
}

/// The whole-value facts that reconcile by picking one source (not by merge).
/// Allergens and dietary are excluded on purpose — they're safety-critical and
/// merge by union / tri-state.
///
/// Derived from the field type rather than listed again: a new `Fact` field is
/// picked up here without anyone remembering to add it.
pub fn picked_facts() -> impl Iterator<Item = ReconcileField> {
    ReconcileField::ALL
        .iter()
        .copied()
        .filter(|f| f.reconciler() == Reconciler::Fact)
}

/// Combine every source's facts into the one answer to display, honouring any
/// recorded source pick (0035). Nutrition and ingredients take one source's value
/// whole — the pick if set and present, else by precedence; allergens union and
/// dietary tri-state (safety — a pick never applies). Pure.
pub fn merge_facts(by_source: &[SourceFacts], prefs: &FactSourceMap) -> ProductFacts {
    let panels: Vec<(Source, Nutrition)> = by_source
        .iter()
        .filter_map(|s| s.facts.nutrition.clone().map(|n| (s.source, n)))
        .collect();
    let nutrition = pick_source(&panels, prefs.get(&ReconcileField::Nutrition))
        .cloned()
        .or_else(|| merge_nutrition(panels.clone()));

    let texts: Vec<(Source, String)> = by_source
        .iter()
        .filter_map(|s| s.facts.ingredients.clone().map(|t| (s.source, t)))
        .collect();
    let ingredients = pick_source(&texts, prefs.get(&ReconcileField::Ingredients))
        .cloned()
        .or_else(|| merge_ingredients(texts.clone()));

    let allergens = merge_allergens(
        by_source
            .iter()
            .flat_map(|s| s.facts.allergens.iter().map(|a| (s.source, a.clone())))
            .collect(),
    );
    let dietary = merge_dietary(
        by_source
            .iter()
            .flat_map(|s| s.facts.dietary.iter().cloned())
            .collect(),
    );
    ProductFacts {
        nutrition,
        ingredients,
        allergens,
        dietary,
    }
}

/// The value from the picked source, if that pick is set and that source actually
/// has a value here. `None` falls the caller back to precedence.
fn pick_source<'a, T>(values: &'a [(Source, T)], pref: Option<&Source>) -> Option<&'a T> {
    let want = *pref?;
    values.iter().find(|(src, _)| *src == want).map(|(_, v)| v)
}

/// Facts that reconcile by source-pick and where the sources genuinely disagree,
/// as `FieldDivergence`s to fold into the same approve grammar as the scalar
/// fields. A pick already recorded (in `prefs`) settles it. Pure — the unit under
/// test.
pub fn fact_divergences(by_source: &[SourceFacts], prefs: &FactSourceMap) -> Vec<FieldDivergence> {
    let mut out = Vec::new();
    for field in picked_facts() {
        // Once a source is picked for this fact, the divergence is settled.
        if prefs.contains_key(&field) {
            continue;
        }
        // Each source's display value for this fact, in precedence order.
        let offered: Vec<(Source, String)> = by_source
            .iter()
            .filter_map(|s| fact_display(field, &s.facts).map(|v| (s.source, v)))
            .collect();
        // Only a real disagreement (≥2 distinct values) is worth approving.
        let distinct: BTreeSet<&str> = offered.iter().map(|(_, v)| v.as_str()).collect();
        if distinct.len() < 2 {
            continue;
        }
        // The current pick is the precedence winner (offered is already ranked).
        let current = offered.first().map(|(_, v)| v.clone());
        let candidates: Vec<Candidate> = offered
            .into_iter()
            .filter(|(_, v)| current.as_deref() != Some(v.as_str()))
            .map(|(source, value)| Candidate { source, value })
            .collect();
        out.push(FieldDivergence {
            field,
            label: field.label().to_string(),
            current,
            candidates,
        });
    }
    out
}

/// One source's display string for a picked fact, or `None` if it has none.
///
/// `None` for a field that isn't a picked fact: those reconcile by another
/// mechanism entirely and have no single value to show in a radio row.
pub(super) fn fact_display(field: ReconcileField, facts: &ProductFacts) -> Option<String> {
    match field {
        ReconcileField::Nutrition => facts.nutrition.as_ref().map(summarize_nutrition),
        ReconcileField::Ingredients => facts
            .ingredients
            .as_ref()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty()),
        ReconcileField::Name
        | ReconcileField::Brand
        | ReconcileField::QuantityLabel
        | ReconcileField::Picture => None,
    }
}
