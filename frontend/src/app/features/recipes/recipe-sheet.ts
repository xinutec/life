import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';

import { onlineHint } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import { SheetHeader } from '../../shared/sheet-header';
import { LifeApi } from '../../life-api';
import { Recipe, RecipeIngredient } from '../../models';

/** Open the sheet in edit mode by passing an existing recipe; omit for a new
 *  one. */
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
  return { name: '', quantity: null, unit: null };
}

/** Add / edit-recipe bottom sheet. Online-only (recipes are a server API);
 *  dismisses with `true` after a successful save so the parent reloads. Edit
 *  mode is driven by an optional `RecipeSheetData` — same form, PUT instead of
 *  POST. */
@Component({
  selector: 'app-recipe-sheet',
  templateUrl: './recipe-sheet.html',
  styleUrl: './recipe-sheet.scss',
  imports: [
    FormsModule,
    MatButtonModule,
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
  private feedback = inject(Feedback);

  /** null = creating; a number = editing that recipe (PUT). */
  private readonly editId = this.data?.recipe.id ?? null;
  readonly editing = this.editId !== null;
  readonly saving = signal(false);

  // Signal-backed form (zoneless: a signal write — incl. from the async save
  // callback — is what refreshes the view). Seeded from the recipe when editing.
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
      // Always leave a row to type into if the recipe had none.
      ingredients: r.ingredients.length ? r.ingredients.map((g) => ({ ...g })) : [blankIngredient()],
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
      this.editId === null
        ? this.api.createRecipe(body)
        : this.api.updateRecipe(this.editId, body);
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
