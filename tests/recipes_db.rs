//! Recipes against a real MariaDB. Runs only when LIFE_TEST_DATABASE_URL is
//! set (see scripts/dev-db.sh); skips otherwise.

use life::db;
use life::inventory::repo as inv_repo;
use life::inventory::types::{ItemCategory, NewItem};
use life::recipes::matching::shopping_list;
use life::recipes::repo;
use life::recipes::types::{NewRecipe, RecipeIngredient};

fn ing(name: &str, qty: Option<f64>, unit: Option<&str>) -> RecipeIngredient {
    RecipeIngredient {
        name: name.into(),
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
