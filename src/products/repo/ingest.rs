//! Writing one source's account of a product: the decisions in
//! [`crate::products::ingest`], applied in one transaction.

use anyhow::{Result, anyhow};
use sqlx::{MySqlConnection, MySqlPool};

use super::facts::{replace_dietary_in, store_facts_in};
use super::{ListingFields, get_by_id, listings_for, record_price, upsert_listing};
use crate::products::ids::{ListingId, ProductId};
use crate::products::ingest::{
    FactsUpdate, Held, SourceAccount, Write, best_name, canonical_writes,
};
use crate::products::types::Product;

/// Take in one source's account of a product, and the picture already fetched
/// for it (see [`crate::products::ingest::picture_to_fetch`]). Returns the
/// canonical product as it now stands.
///
/// One transaction, the canonical row locked: the row, its listing, the
/// picture, the facts and the price commit together or not at all.
pub async fn ingest(
    pool: &MySqlPool,
    account: &SourceAccount,
    picture: Option<(Vec<u8>, String)>,
) -> Result<Product> {
    let mut tx = pool.begin().await?;
    let (id, single_owner) = canonical_row(&mut tx, account).await?;
    let held = held(&mut tx, id, single_owner).await?;
    apply(&mut tx, id, account, &canonical_writes(&held, account)).await?;

    let fields = ListingFields {
        raw_name: account.name.as_deref(),
        brand: account.brand.as_deref(),
        quantity_label: account.quantity_label.as_deref(),
        url: account.url.as_deref(),
        image_url: account.image_url.as_deref(),
        raw_json: account.raw_json.as_deref(),
    };
    upsert_listing(&mut *tx, id, account.source, &account.external_id, &fields).await?;

    if let Some((bytes, mime)) = picture {
        // Only into an empty slot: a picture that arrived meanwhile (an upload)
        // is not replaced by one fetched for a product that had none.
        sqlx::query(
            "UPDATE products SET image = ?, image_mime = ?, image_source = ?, \
             fetched_at = CURRENT_TIMESTAMP WHERE id = ? AND image IS NULL",
        )
        .bind(bytes)
        .bind(mime)
        .bind(account.source)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    match &account.facts {
        FactsUpdate::None => {}
        FactsUpdate::Dietary(flags) => {
            replace_dietary_in(&mut tx, id, flags, account.source).await?;
        }
        FactsUpdate::Full(facts) => store_facts_in(&mut tx, id, facts, account.source).await?,
    }
    if let Some(price) = &account.price {
        let (listing,): (ListingId,) =
            sqlx::query_as("SELECT id FROM product_listings WHERE source = ? AND external_id = ?")
                .bind(account.source)
                .bind(&account.external_id)
                .fetch_one(&mut *tx)
                .await?;
        record_price(&mut *tx, listing, price).await?;
    }
    name_if_unnamed(&mut tx, id).await?;
    tx.commit().await?;
    get_by_id(pool, id)
        .await?
        .ok_or_else(|| anyhow!("product {id} vanished after its ingest"))
}

/// The canonical row this account lands on, and whether it is a barcodeless
/// product only this source lists. A barcode finds or creates the shared row;
/// without one, this source's own listing finds its row, else a new one.
async fn canonical_row(
    conn: &mut MySqlConnection,
    account: &SourceAccount,
) -> Result<(ProductId, bool)> {
    if let Some(barcode) = &account.barcode {
        // Find-or-create in one statement: on the duplicate key,
        // `LAST_INSERT_ID(id)` hands back the existing row's id.
        let res = sqlx::query(
            "INSERT INTO products (barcode, name, brand, source, name_source) \
             VALUES (?, ?, ?, ?, ?) ON DUPLICATE KEY UPDATE id = LAST_INSERT_ID(id)",
        )
        .bind(barcode)
        .bind(account.name.as_deref())
        .bind(account.brand.as_deref())
        .bind(account.source)
        .bind(account.source)
        .execute(&mut *conn)
        .await?;
        return Ok((ProductId::from(res.last_insert_id()), false));
    }
    let listed: Option<(ProductId,)> = sqlx::query_as(
        "SELECT product_id FROM product_listings WHERE source = ? AND external_id = ?",
    )
    .bind(account.source)
    .bind(&account.external_id)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some((id,)) = listed {
        return Ok((id, true));
    }
    // First sighting of a barcodeless product. The origin source and id stay on
    // the row too (vestigial, for the single-source case) so `Product` reports
    // them.
    let res = sqlx::query(
        "INSERT INTO products (name, brand, source, name_source, external_id) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(account.name.as_deref())
    .bind(account.brand.as_deref())
    .bind(account.source)
    .bind(account.source)
    .bind(&account.external_id)
    .execute(&mut *conn)
    .await?;
    Ok((ProductId::from(res.last_insert_id()), true))
}

/// What the row holds now, read under the transaction's lock.
async fn held(conn: &mut MySqlConnection, id: ProductId, single_owner: bool) -> Result<Held> {
    // `<=>` (null-safe equality) so an unset provenance reads as 0, not NULL.
    let (name, brand, quantity_label, name_ours, brand_ours): (
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT name, brand, quantity_label, (name_source <=> 'user'), \
         (brand_source <=> 'user') FROM products WHERE id = ? FOR UPDATE",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(Held {
        name,
        brand,
        quantity_label,
        name_ours: name_ours != 0,
        brand_ours: brand_ours != 0,
        single_owner,
    })
}

/// Apply the decided writes, each recording which source the value came from.
async fn apply(
    conn: &mut MySqlConnection,
    id: ProductId,
    account: &SourceAccount,
    writes: &crate::products::ingest::CanonicalWrites,
) -> Result<()> {
    // Static SQL per column: sqlx takes only literal statements.
    let columns: [(&Write, &'static str); 3] = [
        (
            &writes.name,
            "UPDATE products SET name = ?, name_source = ? WHERE id = ?",
        ),
        (
            &writes.brand,
            "UPDATE products SET brand = ?, brand_source = ? WHERE id = ?",
        ),
        (
            &writes.quantity_label,
            "UPDATE products SET quantity_label = ?, quantity_label_source = ? WHERE id = ?",
        ),
    ];
    for (write, sql) in columns {
        if let Some(value) = write {
            sqlx::query(sql)
                .bind(value.as_deref())
                .bind(account.source)
                .bind(id)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

/// Give a product with no name the best-ranked listing's ([`best_name`]). Never
/// overwrites: a later disagreeing title is a divergence to approve.
async fn name_if_unnamed(conn: &mut MySqlConnection, id: ProductId) -> Result<()> {
    let (name,): (Option<String>,) = sqlx::query_as("SELECT name FROM products WHERE id = ?")
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    if name.is_some_and(|n| !n.trim().is_empty()) {
        return Ok(());
    }
    let listings = listings_for(&mut *conn, id).await?;
    if let Some((name, source)) = best_name(&listings) {
        sqlx::query("UPDATE products SET name = ?, name_source = ? WHERE id = ?")
            .bind(name)
            .bind(source)
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}
