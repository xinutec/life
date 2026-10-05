//! Product facts and the source documents they came from. Stored per source and
//! merged on read: precedence for nutrition and ingredients, union for allergens.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::Result;
use sqlx::types::Json;
use sqlx::{MySqlConnection, MySqlPool};

use crate::products::ids::{AllergenId, ProductId};
use crate::products::nutrition::{
    Allergen, Basis, Claim, Diet, DietaryFlag, Nutrition, Presence, ProductFacts, allergen_list,
    fact_rank, merge_allergens, merge_dietary, merge_ingredients, merge_nutrition,
    summarize_nutrition,
};
use crate::products::source::Source;
use crate::products::types::{
    Candidate, DocKind, FieldDivergence, ReconcileField, Reconciler, SourceDocument, SourceFacts,
};

#[derive(sqlx::FromRow)]
struct NutritionRow {
    source: Source,
    basis: Basis,
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

/// Keyed by product and source (0033), so panels from two sources coexist.
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
    .bind(n.basis)
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

/// Replace this source's allergens, leaving others'. One transaction: half a
/// replace would read as "free from", a health error.
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
    // The key is (product, source, allergen).
    let allergens = allergen_list(allergens.iter().map(|a| (a.allergen.clone(), a.presence)));
    for a in allergens {
        sqlx::query(
            "INSERT INTO product_allergens (product_id, allergen, presence, source) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(product_id)
        .bind(&a.allergen)
        .bind(a.presence)
        .bind(source)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Replace this source's dietary flags, leaving others' (0028). Atomic: a missing
/// "no" reads as an unopposed "yes".
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

pub(super) async fn replace_dietary_in(
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
        .bind(f.flag)
        .bind(f.value)
        .bind(source)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Keep a fetched payload verbatim (0034), so it is never fetched twice;
/// re-fetching the same kind overwrites it.
pub async fn upsert_document(
    pool: &MySqlPool,
    product_id: ProductId,
    source: Source,
    kind: DocKind,
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

pub async fn get_document(
    pool: &MySqlPool,
    product_id: ProductId,
    source: Source,
    kind: DocKind,
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

/// Metadata only, no bodies.
pub async fn documents_for(pool: &MySqlPool, product_id: ProductId) -> Result<Vec<SourceDocument>> {
    let rows: Vec<SourceDocument> = sqlx::query_as(
        "SELECT source, kind, \
         fetched_at, \
         CAST(LENGTH(body) AS SIGNED) AS bytes \
         FROM product_documents WHERE product_id = ? ORDER BY source, kind",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// A source's full fact set. Nutrition and ingredients it lacks stay; allergens
/// and dietary flags replace, as their absence means something. One transaction,
/// or Asda's nutrition could sit beside OFF's allergens attributed to Asda.
pub async fn store_facts(
    pool: &MySqlPool,
    product_id: ProductId,
    facts: &ProductFacts,
    source: Source,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    store_facts_in(&mut tx, product_id, facts, source).await?;
    tx.commit().await?;
    Ok(())
}

/// [`store_facts`] on the caller's connection.
pub(super) async fn store_facts_in(
    conn: &mut MySqlConnection,
    product_id: ProductId,
    facts: &ProductFacts,
    source: Source,
) -> Result<()> {
    if let Some(n) = &facts.nutrition {
        upsert_nutrition_in(&mut *conn, product_id, n, source).await?;
    }
    if let Some(ing) = &facts.ingredients {
        set_ingredients_in(&mut *conn, product_id, ing, source).await?;
    }
    replace_allergens_in(&mut *conn, product_id, &facts.allergens, source).await?;
    replace_dietary_in(&mut *conn, product_id, &facts.dietary, source).await?;
    Ok(())
}

pub async fn facts_for(pool: &MySqlPool, product_id: ProductId) -> Result<ProductFacts> {
    let by_source = facts_by_source(pool, product_id).await?;
    let prefs = fact_source_prefs(pool, product_id).await?;
    Ok(merge_facts(&by_source, &prefs))
}

/// Each source's own facts, in precedence order (retailer before crowd): the raw
/// material for both the merge and the divergences.
pub async fn facts_by_source(pool: &MySqlPool, product_id: ProductId) -> Result<Vec<SourceFacts>> {
    let mut conn = pool.acquire().await?;
    facts_by_source_in(&mut conn, product_id).await
}

pub(super) async fn facts_by_source_in(
    conn: &mut MySqlConnection,
    product_id: ProductId,
) -> Result<Vec<SourceFacts>> {
    let nrows: Vec<NutritionRow> = sqlx::query_as(
        "SELECT source, basis, serving_size, energy_kj, energy_kcal, fat_g, saturates_g, \
         carbohydrate_g, sugars_g, fibre_g, protein_g, salt_g, extra \
         FROM product_nutrition WHERE product_id = ?",
    )
    .bind(product_id)
    .fetch_all(&mut *conn)
    .await?;
    let ing_rows: Vec<(Source, String)> =
        sqlx::query_as("SELECT source, text FROM product_ingredients WHERE product_id = ?")
            .bind(product_id)
            .fetch_all(&mut *conn)
            .await?;
    let allergen_rows: Vec<(Source, AllergenId, Presence)> = sqlx::query_as(
        "SELECT source, allergen, presence FROM product_allergens WHERE product_id = ? \
         ORDER BY allergen",
    )
    .bind(product_id)
    .fetch_all(&mut *conn)
    .await?;
    let dietary_rows: Vec<(Source, Diet, Claim)> = sqlx::query_as(
        "SELECT source, flag, value FROM product_dietary_flags WHERE product_id = ? \
         ORDER BY flag",
    )
    .bind(product_id)
    .fetch_all(&mut *conn)
    .await?;

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
    // An unknown stored value fails the query rather than defaulting; an older
    // allergen name reads as its OFF id, so "wheat" and "barley" are one gluten.
    let mut allergens: BTreeMap<Source, Vec<(AllergenId, Presence)>> = BTreeMap::new();
    for (source, allergen, presence) in allergen_rows {
        allergens
            .entry(source)
            .or_default()
            .push((allergen, presence));
    }
    for (source, claims) in allergens {
        by_source.entry(source).or_insert_with(blank).allergens = allergen_list(claims);
    }
    for (source, flag, value) in dietary_rows {
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
    out.sort_by_key(|s| (fact_rank(s.source), s.source));
    Ok(out)
}

pub type FactSourceMap = HashMap<ReconcileField, Source>;

/// Empty when none: precedence decides.
pub async fn fact_source_prefs(pool: &MySqlPool, product_id: ProductId) -> Result<FactSourceMap> {
    // An unknown kind fails: dropping it would un-settle a decision.
    let rows: Vec<(ReconcileField, Source)> =
        sqlx::query_as("SELECT kind, source FROM product_fact_sources WHERE product_id = ?")
            .bind(product_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().collect())
}

/// The facts settled by picking one source. Allergens and diets are not: they are
/// safety facts and merge. Derived from the type, so a new one joins itself.
pub fn picked_facts() -> impl Iterator<Item = ReconcileField> {
    ReconcileField::ALL
        .iter()
        .copied()
        .filter(|f| f.reconciler() == Reconciler::Fact)
}

/// The facts to show: a pick or precedence for nutrition and ingredients, union
/// and tri-state for allergens and diets, which a pick never touches. Pure.
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

/// The picked source's value, if it has one; `None` falls back to precedence.
fn pick_source<'a, T>(values: &'a [(Source, T)], pref: Option<&Source>) -> Option<&'a T> {
    let want = *pref?;
    values.iter().find(|(src, _)| *src == want).map(|(_, v)| v)
}

/// The picked facts the sources really disagree on, as divergences. A recorded
/// pick settles one. Pure.
pub fn fact_divergences(by_source: &[SourceFacts], prefs: &FactSourceMap) -> Vec<FieldDivergence> {
    let mut out = Vec::new();
    for field in picked_facts() {
        if prefs.contains_key(&field) {
            continue;
        }
        let offered: Vec<(Source, String)> = by_source
            .iter()
            .filter_map(|s| fact_display(field, &s.facts).map(|v| (s.source, v)))
            .collect();
        let distinct: BTreeSet<&str> = offered.iter().map(|(_, v)| v.as_str()).collect();
        if distinct.len() < 2 {
            continue;
        }
        // The current pick is the precedence winner.
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

/// `None` for a field that is not a picked fact.
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
