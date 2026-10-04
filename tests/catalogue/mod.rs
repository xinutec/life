//! A catalogue product for tests that need one, made the way production makes
//! it: through `repo::ingest`. Only the test files that use it include it.

use life::products::ids::{Barcode, ExternalId};
use life::products::ingest::{FactsUpdate, SourceAccount};
use life::products::repo;
use life::products::source::Source;
use life::products::types::Product;

/// A product as an Open Food Facts lookup leaves it: the barcode's canonical
/// row and OFF's listing, filling only what the row does not hold, and the
/// picture if one is given and the row has none.
pub(crate) async fn looked_up(
    pool: &sqlx::MySqlPool,
    barcode: &Barcode,
    name: Option<&str>,
    brand: Option<&str>,
    quantity: Option<&str>,
    picture: Option<(Vec<u8>, String)>,
) -> Product {
    let account = SourceAccount {
        source: Source::Off,
        external_id: ExternalId::from(barcode),
        barcode: Some(barcode.clone()),
        name: name.map(Into::into),
        brand: brand.map(Into::into),
        quantity_label: quantity.map(Into::into),
        url: None,
        image_url: None,
        raw_json: None,
        price: None,
        facts: FactsUpdate::None,
    };
    repo::ingest(pool, &account, picture)
        .await
        .expect("a test product")
}
