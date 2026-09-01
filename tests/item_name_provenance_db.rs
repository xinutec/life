//! Which name an item shows, and why. Real MariaDB; runs only when
//! LIFE_TEST_DATABASE_URL is set.
//!
//! The rule under test is provenance, not precedence. The read path used to be
//! `COALESCE(p.name, i.name, '')`, so the catalogue won whenever it had anything
//! — including an Open Food Facts record whose "name" is a marketing sentence.
//! Measured on the live data before this change: an item typed as "Oregano"
//! displayed as "GENTLY DRIED TO HELP PRESERVE THE NATAL GRUNA FLAV", and there
//! was no way to overrule it.

mod common;

use life::db;
use life::inventory::repo;
use life::inventory::types::{ItemCategory, ItemNameSource, NewItem};
use life::products::ids::ProductId;

async fn product(pool: &sqlx::MySqlPool, barcode: &str, name: &str) -> ProductId {
    sqlx::query("INSERT INTO products (barcode, name) VALUES (?, ?) ON DUPLICATE KEY UPDATE name = VALUES(name)")
        .bind(barcode)
        .bind(name)
        .execute(pool)
        .await
        .expect("insert product");
    let (id,): (u64,) = sqlx::query_as("SELECT id FROM products WHERE barcode = ?")
        .bind(barcode)
        .fetch_one(pool)
        .await
        .expect("product id");
    ProductId::from(id)
}

fn item(name: &str, product_id: Option<ProductId>) -> NewItem {
    NewItem {
        name: name.into(),
        product_id,
        category: ItemCategory::Other,
        quantity: None,
        unit: None,
        expiry: None,
        expiry_precision: None,
        location_id: None,
        barcode: None,
        name_source: None,
    }
}

/// The item form's save when somebody has edited the name field: the client that
/// owns the form is the only place that knows the field was touched.
fn renamed(name: &str, product_id: Option<ProductId>) -> NewItem {
    NewItem {
        name_source: Some(ItemNameSource::User),
        ..item(name, product_id)
    }
}

#[tokio::test]
async fn a_typed_name_outranks_the_catalogue_but_a_left_alone_one_follows_it() {
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let user = "test-user-item-names";
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean");

    // The shape that motivated this: a catalogue name nobody would choose.
    let shouty = product(
        &pool,
        "T-NAME-1",
        "GENTLY DRIED TO HELP PRESERVE THE FLAVOUR",
    )
    .await;
    // And the shape where the catalogue is plainly better.
    let proper = product(&pool, "T-NAME-2", "Cooks' Ingredients Black Peppercorns").await;

    let typed = repo::create_item(&pool, user, item("Oregano", Some(shouty)))
        .await
        .expect("create typed");
    // A name supplied while ADDING is a scribble — you type a word and then
    // scan. catalog_db.rs has always required the catalogue to win here.
    assert_eq!(
        repo::get_item(&pool, user, typed.id)
            .await
            .unwrap()
            .unwrap()
            .name,
        "GENTLY DRIED TO HELP PRESERVE THE FLAVOUR",
        "a name typed while adding must NOT override the catalogue"
    );
    // Renaming it is the act of naming, and only the form knows it happened.
    repo::update_item(&pool, user, typed.id, renamed("Oregano", Some(shouty)))
        .await
        .expect("rename");
    let left = repo::create_item(
        &pool,
        user,
        item("Cooks' Ingredients Black Peppercorns", Some(proper)),
    )
    .await
    .expect("create left-alone");

    let got = repo::get_item(&pool, user, typed.id)
        .await
        .expect("read")
        .expect("present");
    assert_eq!(
        got.name, "Oregano",
        "a deliberately typed name must survive the catalogue"
    );

    let got = repo::get_item(&pool, user, left.id)
        .await
        .expect("read")
        .expect("present");
    assert_eq!(got.name, "Cooks' Ingredients Black Peppercorns");

    // The property the old COALESCE had and which must NOT be lost: correcting a
    // product reaches items already in the cupboard, with no refresh step.
    sqlx::query("UPDATE products SET name = ? WHERE id = ?")
        .bind("Cooks' Ingredients Black Peppercorns 100g")
        .bind(proper)
        .execute(&pool)
        .await
        .expect("correct the product");
    let got = repo::get_item(&pool, user, left.id)
        .await
        .expect("read")
        .expect("present");
    assert_eq!(
        got.name, "Cooks' Ingredients Black Peppercorns 100g",
        "a correction must still reach an item that never had an opinion"
    );

    // ...and must NOT reach one that does.
    sqlx::query("UPDATE products SET name = ? WHERE id = ?")
        .bind("SOMETHING ELSE ENTIRELY")
        .bind(shouty)
        .execute(&pool)
        .await
        .expect("change the shouty product");
    let got = repo::get_item(&pool, user, typed.id)
        .await
        .expect("read")
        .expect("present");
    assert_eq!(
        got.name, "Oregano",
        "a correction must leave an authored name alone"
    );

    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean up");
}

#[tokio::test]
async fn an_override_survives_a_save_that_says_nothing_and_is_cleared_by_one_that_does() {
    // The two halves of "absent means no statement".
    //
    // Every caller that is not the item form — sync, a script, the Android app —
    // sends no name_source at all. If absent were read as 'product', any of them
    // re-saving an item would silently strip a name its owner chose to keep, and
    // the loss would look like the catalogue simply winning. So absent preserves.
    //
    // Which then makes clearing an override its own explicit act, rather than
    // something that falls out of retyping the catalogue's name.
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let user = "test-user-item-names-unoverride";
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean");

    let pid = product(&pool, "T-NAME-3", "Table Salt 750g").await;
    let it = repo::create_item(&pool, user, item("My salt", Some(pid)))
        .await
        .expect("create");
    repo::update_item(&pool, user, it.id, renamed("My salt", Some(pid)))
        .await
        .expect("rename");
    assert_eq!(
        repo::get_item(&pool, user, it.id)
            .await
            .unwrap()
            .unwrap()
            .name,
        "My salt"
    );

    // A save that states nothing must not disturb the choice.
    repo::update_item(&pool, user, it.id, item("My salt", Some(pid)))
        .await
        .expect("silent save");
    assert_eq!(
        repo::get_item(&pool, user, it.id)
            .await
            .unwrap()
            .unwrap()
            .name,
        "My salt",
        "a caller that says nothing about the name must not clear the override"
    );

    // Saying so explicitly hands the name back to the catalogue, and a later
    // correction reaches it again.
    let back = NewItem {
        name_source: Some(ItemNameSource::Product),
        ..item("My salt", Some(pid))
    };
    repo::update_item(&pool, user, it.id, back)
        .await
        .expect("clear");
    sqlx::query("UPDATE products SET name = ? WHERE id = ?")
        .bind("Table Salt 1kg")
        .bind(pid)
        .execute(&pool)
        .await
        .expect("correct");
    assert_eq!(
        repo::get_item(&pool, user, it.id)
            .await
            .unwrap()
            .unwrap()
            .name,
        "Table Salt 1kg",
        "after handing the name back, corrections reach the item again"
    );

    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean up");
}

#[tokio::test]
async fn the_trash_shows_an_item_under_the_name_it_was_last_seen_under() {
    // The cupboard and the trash resolve the name in two separate queries, and
    // they must agree: you look for a deleted thing by the name you last saw it
    // under, so a row that renames itself on the way into the trash is a row you
    // cannot find. This was the second copy of the old COALESCE, and grepping for
    // it is what turned it up — fixing only the cupboard would have left the
    // belief alive here.
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let user = "test-user-item-names-trash";
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean");

    let pid = product(
        &pool,
        "T-NAME-4",
        "GENTLY DRIED TO HELP PRESERVE THE FLAVOUR",
    )
    .await;
    let it = repo::create_item(&pool, user, item("Oregano", Some(pid)))
        .await
        .expect("create");
    repo::update_item(&pool, user, it.id, renamed("Oregano", Some(pid)))
        .await
        .expect("rename");
    let seen_as = repo::get_item(&pool, user, it.id)
        .await
        .unwrap()
        .unwrap()
        .name;
    assert_eq!(seen_as, "Oregano");

    repo::delete_item(&pool, user, it.id).await.expect("delete");

    let trash = life::trash::repo::list(&pool, user).await.expect("trash");
    let entry = trash
        .iter()
        .find(|e| e.ref_ == it.id.to_string())
        .expect("the deleted item is in the trash");
    assert_eq!(
        entry.name, seen_as,
        "the trash must name it exactly as the cupboard did"
    );

    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean up");
}
