//! Cooking a recipe against a real MariaDB: the cupboard really goes down, the
//! audit really records it, and the lines it couldn't settle really come back.
//! Runs only when LIFE_TEST_DATABASE_URL is set; skips otherwise.

use life::db;
use life::inventory::repo as inv;
use life::inventory::types::{ItemCategory, NewItem};
use life::recipes::cooking::{LineOutcome, Take, Untouched};
use life::recipes::repo;
use life::recipes::types::{NewRecipe, RecipeIngredient};

async fn connect() -> Option<sqlx::MySqlPool> {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping cook DB test");
        return None;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    Some(pool)
}

fn stock(name: &str, quantity: Option<f64>, unit: Option<&str>) -> NewItem {
    NewItem {
        name: name.into(),
        category: ItemCategory::Food,
        quantity,
        unit: unit.map(Into::into),
        expiry: None,
        location_id: None,
        barcode: None,
        product_id: None,
    }
}

fn ing(name: &str, quantity: Option<f64>, unit: Option<&str>) -> RecipeIngredient {
    RecipeIngredient {
        name: name.into(),
        product_id: None,
        product_name: None,
        quantity,
        unit: unit.map(Into::into),
    }
}

#[tokio::test]
async fn cooking_takes_the_recipe_out_of_the_cupboard() {
    let Some(pool) = connect().await else { return };
    let user = "test-user-cook";
    for sql in [
        "DELETE FROM items WHERE user_id = ?",
        "DELETE FROM recipes WHERE user_id = ?",
    ] {
        sqlx::query(sql).bind(user).execute(&pool).await.unwrap();
    }

    let flour = inv::create_item(&pool, user, stock("flour", Some(950.0), Some("g")))
        .await
        .unwrap();
    // A jar the recipe wants in grams, and a line with no amount at all: both
    // must come back reported rather than quietly ignored.
    let cumin = inv::create_item(&pool, user, stock("cumin", Some(1.0), Some("jar")))
        .await
        .unwrap();
    inv::create_item(&pool, user, stock("salt", None, None))
        .await
        .unwrap();

    let recipe = repo::create_recipe(
        &pool,
        user,
        NewRecipe {
            name: "Flatbread".into(),
            instructions: None,
            servings: None,
            ingredients: vec![
                ing("flour", Some(200.0), Some("g")),
                ing("cumin", Some(2.0), Some("g")),
                ing("salt", None, None),
                ing("saffron", Some(1.0), Some("pinch")),
            ],
        },
    )
    .await
    .unwrap();

    let lines = repo::cook_recipe(&pool, user, recipe.id)
        .await
        .unwrap()
        .expect("the recipe exists");
    assert_eq!(lines.len(), 4, "every ingredient is accounted for");

    let by_name = |n: &str| {
        lines
            .iter()
            .find(|l| l.ingredient == n)
            .unwrap_or_else(|| panic!("no line for {n}"))
            .outcome
            .clone()
    };
    assert!(matches!(by_name("flour"), LineOutcome::Took { .. }));
    assert_eq!(
        by_name("cumin"),
        LineOutcome::Untouched {
            why: Untouched::NoComparableStock
        }
    );
    assert_eq!(
        by_name("salt"),
        LineOutcome::Untouched {
            why: Untouched::NoAmount
        }
    );
    assert_eq!(
        by_name("saffron"),
        LineOutcome::Untouched {
            why: Untouched::NoStock
        }
    );

    // The cupboard moved for exactly the one line that could be settled.
    assert_eq!(
        inv::get_item(&pool, user, flour.id)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        Some(750.0)
    );
    assert_eq!(
        inv::get_item(&pool, user, cumin.id)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        Some(1.0),
        "the jar is untouched — grams were never taken out of it"
    );

    // And the audit says what went, so a rate can be derived later.
    let (n, total): (i64, Option<f64>) = sqlx::query_as(
        "SELECT COUNT(*), SUM(quantity) FROM item_history WHERE item_id = ? AND event = 'used'",
    )
    .bind(flour.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((n, total), (1, Some(200.0)));
}

#[tokio::test]
async fn cooking_never_leaves_a_negative_amount() {
    let Some(pool) = connect().await else { return };
    let user = "test-user-cook-short";
    for sql in [
        "DELETE FROM items WHERE user_id = ?",
        "DELETE FROM recipes WHERE user_id = ?",
    ] {
        sqlx::query(sql).bind(user).execute(&pool).await.unwrap();
    }

    let flour = inv::create_item(&pool, user, stock("flour", Some(150.0), Some("g")))
        .await
        .unwrap();
    let recipe = repo::create_recipe(
        &pool,
        user,
        NewRecipe {
            name: "Big bake".into(),
            instructions: None,
            servings: None,
            ingredients: vec![ing("flour", Some(500.0), Some("g"))],
        },
    )
    .await
    .unwrap();

    let lines = repo::cook_recipe(&pool, user, recipe.id)
        .await
        .unwrap()
        .unwrap();
    // Compared through the enum's derived PartialEq rather than a bare float:
    // 500 - 150 is exact in binary, and going via the variant also pins that the
    // row was drained rather than skipped.
    assert_eq!(
        lines[0].outcome,
        LineOutcome::Short {
            from: vec![Take {
                item_id: flour.id,
                name: "flour".into(),
                amount: 150.0,
                left: 0.0
            }],
            short: 350.0
        }
    );
    assert_eq!(
        inv::get_item(&pool, user, flour.id)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        Some(0.0),
        "emptied, never negative — the write floors at zero"
    );
}

#[tokio::test]
async fn you_cannot_cook_someone_elses_recipe() {
    let Some(pool) = connect().await else { return };
    let (mine, theirs) = ("test-user-cook-mine", "test-user-cook-theirs");
    for u in [mine, theirs] {
        sqlx::query("DELETE FROM recipes WHERE user_id = ?")
            .bind(u)
            .execute(&pool)
            .await
            .unwrap();
    }
    let recipe = repo::create_recipe(
        &pool,
        theirs,
        NewRecipe {
            name: "Not yours".into(),
            instructions: None,
            servings: None,
            ingredients: vec![],
        },
    )
    .await
    .unwrap();
    assert!(
        repo::cook_recipe(&pool, mine, recipe.id)
            .await
            .unwrap()
            .is_none()
    );
}
