//! Persistence for recipes + their ingredients.

use std::collections::HashMap;

use anyhow::Result;
use sqlx::MySqlPool;

use super::types::{NewRecipe, Recipe, RecipeIngredient};

#[derive(sqlx::FromRow)]
struct RecipeRow {
    id: u64,
    name: String,
    instructions: Option<String>,
    servings: Option<i32>,
}

#[derive(sqlx::FromRow)]
struct IngredientRow {
    recipe_id: u64,
    name: String,
    quantity: Option<f64>,
    unit: Option<String>,
}

/// All recipes for a user, each with its ingredients. One query per table; the
/// ingredients are grouped in memory by recipe id.
pub async fn list_recipes(pool: &MySqlPool, user_id: &str) -> Result<Vec<Recipe>> {
    let recipe_rows: Vec<RecipeRow> = sqlx::query_as(
        "SELECT id, name, instructions, servings FROM recipes \
         WHERE user_id = ? AND deleted_at IS NULL ORDER BY name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let ing_rows: Vec<IngredientRow> = sqlx::query_as(
        "SELECT ri.recipe_id, ri.name, ri.quantity, ri.unit \
         FROM recipe_ingredients ri JOIN recipes r ON r.id = ri.recipe_id \
         WHERE r.user_id = ? AND r.deleted_at IS NULL \
         ORDER BY ri.recipe_id, ri.sort_order, ri.id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut by_recipe: HashMap<u64, Vec<RecipeIngredient>> = HashMap::new();
    for r in ing_rows {
        by_recipe
            .entry(r.recipe_id)
            .or_default()
            .push(RecipeIngredient {
                name: r.name,
                quantity: r.quantity,
                unit: r.unit,
            });
    }

    Ok(recipe_rows
        .into_iter()
        .map(|r| Recipe {
            id: r.id,
            name: r.name,
            instructions: r.instructions,
            servings: r.servings,
            ingredients: by_recipe.remove(&r.id).unwrap_or_default(),
        })
        .collect())
}

pub async fn get_recipe(pool: &MySqlPool, user_id: &str, id: u64) -> Result<Option<Recipe>> {
    let recipe: Option<RecipeRow> = sqlx::query_as(
        "SELECT id, name, instructions, servings FROM recipes \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    let Some(recipe) = recipe else {
        return Ok(None);
    };

    let ing_rows: Vec<IngredientRow> = sqlx::query_as(
        "SELECT recipe_id, name, quantity, unit FROM recipe_ingredients \
         WHERE recipe_id = ? ORDER BY sort_order, id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    Ok(Some(Recipe {
        id: recipe.id,
        name: recipe.name,
        instructions: recipe.instructions,
        servings: recipe.servings,
        ingredients: ing_rows
            .into_iter()
            .map(|r| RecipeIngredient {
                name: r.name,
                quantity: r.quantity,
                unit: r.unit,
            })
            .collect(),
    }))
}

/// Create a recipe and its ingredients atomically.
pub async fn create_recipe(pool: &MySqlPool, user_id: &str, new: NewRecipe) -> Result<Recipe> {
    let mut tx = pool.begin().await?;
    let res = sqlx::query(
        "INSERT INTO recipes (user_id, name, instructions, servings) VALUES (?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(&new.name)
    .bind(&new.instructions)
    .bind(new.servings)
    .execute(&mut *tx)
    .await?;
    let id = res.last_insert_id();

    for (i, ing) in new.ingredients.iter().enumerate() {
        sqlx::query(
            "INSERT INTO recipe_ingredients (recipe_id, name, quantity, unit, sort_order) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(&ing.name)
        .bind(ing.quantity)
        .bind(&ing.unit)
        .bind(i as i32)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok(Recipe {
        id,
        name: new.name,
        instructions: new.instructions,
        servings: new.servings,
        ingredients: new.ingredients,
    })
}

/// Replace a recipe's fields and its whole ingredient list atomically. Returns
/// the updated recipe, or None if no live recipe with that id belongs to the
/// user (unknown / deleted / someone else's). Ingredients are delete-all +
/// re-insert (the same shape `create_recipe` writes), so the list the client
/// sends is exactly what's stored — no stale rows survive an edit.
pub async fn update_recipe(
    pool: &MySqlPool,
    user_id: &str,
    id: u64,
    new: NewRecipe,
) -> Result<Option<Recipe>> {
    let mut tx = pool.begin().await?;
    // Lock + confirm the row exists for this user (an UPDATE's rows_affected is
    // 0 for an unchanged row too, so it can't stand in for existence).
    let exists: Option<(u64,)> = sqlx::query_as(
        "SELECT id FROM recipes WHERE id = ? AND user_id = ? AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    if exists.is_none() {
        return Ok(None);
    }

    sqlx::query("UPDATE recipes SET name = ?, instructions = ?, servings = ? WHERE id = ?")
        .bind(&new.name)
        .bind(&new.instructions)
        .bind(new.servings)
        .bind(id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("DELETE FROM recipe_ingredients WHERE recipe_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for (i, ing) in new.ingredients.iter().enumerate() {
        sqlx::query(
            "INSERT INTO recipe_ingredients (recipe_id, name, quantity, unit, sort_order) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(&ing.name)
        .bind(ing.quantity)
        .bind(&ing.unit)
        .bind(i as i32)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok(Some(Recipe {
        id,
        name: new.name,
        instructions: new.instructions,
        servings: new.servings,
        ingredients: new.ingredients,
    }))
}

/// Delete a recipe — a tombstone, restorable from the trash; its ingredient
/// rows stay attached. Returns whether a row was tombstoned.
pub async fn delete_recipe(pool: &MySqlPool, user_id: &str, id: u64) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE recipes SET deleted_at = NOW() \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Restore a deleted recipe. Returns whether a tombstone was cleared.
pub async fn restore_recipe(pool: &MySqlPool, user_id: &str, id: u64) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE recipes SET deleted_at = NULL \
         WHERE id = ? AND user_id = ? AND deleted_at IS NOT NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}
