//! Recipes against a real MariaDB. Runs only when LIFE_TEST_DATABASE_URL is
//! set (see scripts/dev-db.sh); skips otherwise.

use life::db;
use life::inventory::repo as inv_repo;
use life::inventory::types::{ItemCategory, NewItem};
use life::products::repo as prod_repo;
use life::recipes::matching::shopping_list;
use life::recipes::repo;
use life::recipes::types::{NewRecipe, RecipeIngredient};

fn ing(name: &str, qty: Option<f64>, unit: Option<&str>) -> RecipeIngredient {
    RecipeIngredient {
        name: name.into(),
        product_id: None,
        product_name: None,
        quantity: qty,
        unit: unit.map(Into::into),
    }
}

#[tokio::test]
async fn recipe_create_and_shopping_list_against_real_db() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping recipes DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-recipes";
    sqlx::query("DELETE FROM recipes WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    // Stock: cumin, but no salt.
    inv_repo::create_item(
        &pool,
        user,
        NewItem {
            name: "Cumin".into(),
            category: ItemCategory::Food,
            quantity: Some(1.0),
            unit: Some("jar".into()),
            expiry: None,
            location_id: None,
            barcode: None,
            product_id: None,
        },
    )
    .await
    .unwrap();

    let recipe = repo::create_recipe(
        &pool,
        user,
        NewRecipe {
            name: "Dal".into(),
            instructions: Some("Simmer.".into()),
            servings: Some(2),
            ingredients: vec![ing("cumin", None, None), ing("salt", None, None)],
        },
    )
    .await
    .unwrap();

    // Round-trips with both ingredients.
    let fetched = repo::get_recipe(&pool, user, recipe.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.ingredients.len(), 2);

    // Shopping list = the missing salt only.
    let inventory = inv_repo::list_items(&pool, user).await.unwrap();
    let list = shopping_list(&fetched.ingredients, &inventory);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "salt");

    // Edit: rename, drop cumin, add rice — the stored list is exactly the new
    // one (delete-all + re-insert, no stale rows survive).
    let updated = repo::update_recipe(
        &pool,
        user,
        recipe.id,
        NewRecipe {
            name: "Kitchari".into(),
            instructions: Some("Simmer longer.".into()),
            servings: Some(3),
            ingredients: vec![ing("rice", Some(1.0), Some("cup")), ing("salt", None, None)],
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(updated.name, "Kitchari");
    let refetched = repo::get_recipe(&pool, user, recipe.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(refetched.name, "Kitchari");
    assert_eq!(refetched.servings, Some(3));
    let names: Vec<&str> = refetched
        .ingredients
        .iter()
        .map(|i| i.name.as_str())
        .collect();
    assert_eq!(names, vec!["rice", "salt"]); // cumin gone, order preserved

    // Editing an unknown / not-owned recipe is a no-op None, not a phantom write.
    assert!(
        repo::update_recipe(
            &pool,
            "someone-else",
            recipe.id,
            NewRecipe {
                name: "Hijack".into(),
                instructions: None,
                servings: None,
                ingredients: vec![],
            }
        )
        .await
        .unwrap()
        .is_none()
    );

    // Delete the recipe (ingredients cascade).
    assert!(repo::delete_recipe(&pool, user, recipe.id).await.unwrap());
    assert!(
        repo::get_recipe(&pool, user, recipe.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(!repo::delete_recipe(&pool, user, recipe.id).await.unwrap());

    // ...and editing a deleted recipe is likewise None.
    assert!(
        repo::update_recipe(
            &pool,
            user,
            recipe.id,
            NewRecipe {
                name: "Zombie".into(),
                instructions: None,
                servings: None,
                ingredients: vec![],
            }
        )
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn a_linked_ingredient_matches_stock_by_product_not_by_name() {
    // The pure rule is unit-tested in tests/recipes.rs; this is the half only a
    // real DB can answer — that the link survives the write, the re-read, and
    // the delete-all-and-re-insert an edit performs, and that losing the product
    // leaves the line intact and matching by name again (ON DELETE SET NULL).
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping recipe product-link test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-recipe-link";
    let barcode = "5099999900040";
    for sql in [
        "DELETE FROM recipes WHERE user_id = ?",
        "DELETE FROM items WHERE user_id = ?",
    ] {
        sqlx::query(sql).bind(user).execute(&pool).await.unwrap();
    }
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(barcode)
        .execute(&pool)
        .await
        .unwrap();

    prod_repo::upsert(
        &pool,
        barcode,
        Some("Bart Ground Cumin 38g"),
        Some("Bart"),
        Some("38g"),
        None,
    )
    .await
    .unwrap();
    let product = prod_repo::get(&pool, barcode)
        .await
        .unwrap()
        .expect("product");

    // The jar in the cupboard is called what the SHOP calls it.
    inv_repo::create_item(
        &pool,
        user,
        NewItem {
            name: "Bart Ground Cumin 38g".into(),
            category: ItemCategory::Food,
            quantity: Some(1.0),
            unit: Some("jar".into()),
            expiry: None,
            location_id: None,
            barcode: Some(barcode.into()),
            product_id: Some(product.id),
        },
    )
    .await
    .unwrap();

    // The recipe line is called what a COOK calls it — and is linked.
    let mut cumin = ing("cumin", None, None);
    cumin.product_id = Some(product.id);
    let recipe = repo::create_recipe(
        &pool,
        user,
        NewRecipe {
            name: "Dal".into(),
            instructions: None,
            servings: None,
            ingredients: vec![cumin.clone()],
        },
    )
    .await
    .unwrap();

    let fetched = repo::get_recipe(&pool, user, recipe.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.ingredients[0].product_id, Some(product.id));
    // Joined on read, so a client can say WHAT the line points at — the line
    // itself is still called "cumin".
    assert_eq!(fetched.ingredients[0].name, "cumin");
    assert_eq!(
        fetched.ingredients[0].product_name.as_deref(),
        Some("Bart Ground Cumin 38g")
    );
    // And the create response is the read, not an echo of the request.
    assert_eq!(
        recipe.ingredients[0].product_name.as_deref(),
        Some("Bart Ground Cumin 38g")
    );

    let inventory = inv_repo::list_items(&pool, user).await.unwrap();
    assert!(
        shopping_list(&fetched.ingredients, &inventory).is_empty(),
        "the cumin is in the cupboard under the shop's name"
    );

    // An edit re-inserts every line; the link must not be dropped on the way.
    repo::update_recipe(
        &pool,
        user,
        recipe.id,
        NewRecipe {
            name: "Dal".into(),
            instructions: None,
            servings: None,
            ingredients: vec![cumin, ing("salt", None, None)],
        },
    )
    .await
    .unwrap()
    .unwrap();
    let edited = repo::get_recipe(&pool, user, recipe.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(edited.ingredients[0].product_id, Some(product.id));
    assert_eq!(edited.ingredients[1].product_id, None);

    // Losing the product must not take the recipe line with it: the line still
    // names a real ingredient, it just goes back to matching by name.
    sqlx::query("DELETE FROM products WHERE id = ?")
        .bind(product.id)
        .execute(&pool)
        .await
        .unwrap();
    let orphaned = repo::get_recipe(&pool, user, recipe.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(orphaned.ingredients.len(), 2);
    assert_eq!(orphaned.ingredients[0].name, "cumin");
    assert_eq!(orphaned.ingredients[0].product_id, None);
}
