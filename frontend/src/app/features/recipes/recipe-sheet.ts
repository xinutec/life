import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatDialogModule } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { Dialogs } from '@xinutec/ui-scaffold';

import { onlineHint } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import { ProductPick, ProductPickData, ProductPicker } from '../../shared/product-picker';
import { SheetHeader } from '../../shared/sheet-header';
import { LifeApi } from '../../life-api';
import { Recipe, RecipeIngredient } from '../../models';

/** Present when editing. */
export interface RecipeSheetData {
  recipe: Recipe;
}

interface RecipeForm {
  name: string;
  instructions: string | null;
  servings: number | null;
  ingredients: RecipeIngredient[];
}

function blankIngredient(): RecipeIngredient {
  return { name: '', product_id: null, product_name: null, quantity: null, unit: null };
}

/** Add or edit a recipe; dismisses with `true` after a save. */
@Component({
  selector: 'app-recipe-sheet',
  templateUrl: './recipe-sheet.html',
  styleUrl: './recipe-sheet.scss',
  imports: [
    FormsModule,
    MatButtonModule,
    MatDialogModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    SheetHeader,
  ],
})
export class RecipeSheet {
  private ref = inject(MatBottomSheetRef<RecipeSheet, boolean>);
  private data = inject<RecipeSheetData | null>(MAT_BOTTOM_SHEET_DATA, { optional: true });
  private api = inject(LifeApi);
  private dialog = inject(Dialogs);
  private feedback = inject(Feedback);

  private readonly editId = this.data?.recipe.id ?? null;
  readonly editing = this.editId !== null;
  readonly saving = signal(false);

  readonly form = signal<RecipeForm>(this.seed());

  private seed(): RecipeForm {
    const r = this.data?.recipe;
    if (!r) {
      return { name: '', instructions: null, servings: null, ingredients: [blankIngredient()] };
    }
    return {
      name: r.name,
      instructions: r.instructions,
      servings: r.servings,
      ingredients: r.ingredients.length
        ? r.ingredients.map((g) => ({ ...g }))
        : [blankIngredient()],
    };
  }

  patch(p: Partial<RecipeForm>): void {
    this.form.update((f) => ({ ...f, ...p }));
  }
  patchIngredient(i: number, p: Partial<RecipeIngredient>): void {
    this.form.update((f) => ({
      ...f,
      ingredients: f.ingredients.map((g, j) => (j === i ? { ...g, ...p } : g)),
    }));
  }
  /** Link a line to a product, so it matches the jar whatever either is called.
   *  The line keeps its own name. */
  linkProduct(i: number): void {
    this.dialog
      .open<ProductPicker, ProductPickData, ProductPick | null>(ProductPicker, {
        data: { initialQuery: this.form().ingredients[i].name.trim() },
        ariaLabel: 'Find a product',
      })
      .afterClosed()
      .subscribe((pick) => {
        if (!pick) return;
        const row = this.form().ingredients[i];
        // An inventory pick may have no product behind it: keep the name and
        // say nothing was linked.
        this.patchIngredient(i, {
          name: row.name.trim() || pick.name,
          product_id: pick.product_id,
          product_name: pick.product_id ? pick.name : null,
        });
        // The unit only: comparing stock needs one, but a pack is not an amount
        // the recipe needs.
        if (pick.unit != null && !row.unit?.trim()) this.patchIngredient(i, { unit: pick.unit });
        if (!pick.product_id) {
          this.feedback.error(`“${pick.name}” isn’t in the product catalogue, so nothing to link`);
        }
      });
  }

  unlinkProduct(i: number): void {
    this.patchIngredient(i, { product_id: null, product_name: null });
  }

  addIngredientRow(): void {
    this.form.update((f) => ({ ...f, ingredients: [...f.ingredients, blankIngredient()] }));
  }
  removeIngredientRow(i: number): void {
    this.form.update((f) => ({ ...f, ingredients: f.ingredients.filter((_, j) => j !== i) }));
  }

  save(): void {
    const form = this.form();
    if (!form.name.trim() || this.saving()) return;
    this.saving.set(true);
    const body = { ...form, ingredients: form.ingredients.filter((g) => g.name.trim()) };
    const req =
      this.editId === null ? this.api.createRecipe(body) : this.api.updateRecipe(this.editId, body);
    req.subscribe({
      next: () => this.ref.dismiss(true),
      error: (e: unknown) => {
        this.saving.set(false);
        this.feedback.error(`Could not save the recipe${onlineHint(e)}`);
      },
    });
  }

  close(): void {
    this.ref.dismiss();
  }
}
