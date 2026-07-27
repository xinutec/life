import { Component, computed, inject, signal } from '@angular/core';
import { MatBottomSheet, MatBottomSheetModule } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatChipsModule } from '@angular/material/chips';
import { MatIconModule } from '@angular/material/icon';

import { onlineHint } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import { ListState } from '../../shared/list-state';
import { LifeApi } from '../../life-api';
import { CookableStore, RecipesStore } from '../../stores/catalog';
import { ItemCategory, Recipe, RecipeIngredient } from '../../models';
import { ShoppingStore } from '../../sync/shopping-store';
import { RecipeSheet, RecipeSheetData } from './recipe-sheet';

@Component({
  selector: 'app-recipes',
  templateUrl: './recipes.html',
  styleUrl: './recipes.scss',
  imports: [
    MatBottomSheetModule,
    MatCardModule,
    MatButtonModule,
    MatIconModule,
    MatChipsModule,
    ListState,
  ],
})
export class Recipes {
  private api = inject(LifeApi);
  private sheet = inject(MatBottomSheet);
  private feedback = inject(Feedback);
  private recipesStore = inject(RecipesStore);
  private cookableStore = inject(CookableStore);
  private shopping = inject(ShoppingStore);

  /** Online-only writes must not fail into silence: announce and move on. */
  private failed(what: string) {
    return (e: unknown) => {
      this.feedback.error(`Could not ${what}${onlineHint(e)}`);
    };
  }

  // Shared catalogs, retained across tab switches (see CachedResource).
  readonly recipes = computed(() => this.recipesStore.value() ?? []);
  readonly loaded = this.recipesStore.loaded;
  readonly loadError = this.recipesStore.error;
  readonly refreshing = this.recipesStore.refreshing;
  readonly cookableIds = computed(
    () => new Set((this.cookableStore.value() ?? []).map((r) => r.id)),
  );
  /** Per-recipe "what you're short of", loaded on demand by [[loadShoppingList]]. */
  private readonly missingByRecipe = signal<Map<number, RecipeIngredient[]>>(new Map());

  readonly cookableCount = computed(() => this.cookableIds().size);

  /** The FAB's action: the new-recipe sheet; reload after a save. */
  addRecipe(): void {
    this.openSheet();
  }

  /** Edit an existing recipe in the same sheet, seeded from it. */
  editRecipe(recipe: Recipe): void {
    this.openSheet({ recipe });
  }

  private openSheet(data?: RecipeSheetData): void {
    this.sheet
      .open<RecipeSheet, RecipeSheetData | undefined, boolean>(RecipeSheet, { data })
      .afterDismissed()
      .subscribe((saved) => {
        if (saved) this.reload();
      });
  }

  constructor() {
    this.reload();
  }

  reload(): void {
    this.recipesStore.refresh();
    this.cookableStore.refresh();
  }

  deleteRecipe(id: number): void {
    this.api.deleteRecipe(id).subscribe({
      next: () => {
        this.reload();
        // Deletes are tombstones (restorable from Recently deleted); offer an
        // immediate Undo so a fat-finger costs one tap.
        this.feedback.undo('Recipe deleted', () => {
          this.api.restoreTrash('recipe', String(id)).subscribe({
            next: () => this.reload(),
            error: this.failed('undo the delete'),
          });
        });
      },
      error: this.failed('delete the recipe'),
    });
  }

  isCookable(id: number): boolean {
    return this.cookableIds().has(id);
  }

  loadShoppingList(id: number): void {
    this.api.shoppingList(id).subscribe({
      next: (list) => {
        const next = new Map(this.missingByRecipe());
        next.set(id, list);
        this.missingByRecipe.set(next);
      },
      error: this.failed('load the shopping list'),
    });
  }

  shoppingFor(id: number): RecipeIngredient[] | undefined {
    return this.missingByRecipe().get(id);
  }

  /** The Recipe→Buy bridge: everything this recipe needs and the cupboard hasn't
   *  got, onto the Buy list in one tap.
   *
   *  Carries the quantity, unlike the Inventory→Buy bridge — there the number is
   *  what you own, here it is what the recipe is short of, which is exactly what
   *  to buy. Sends the LINE's name, not the linked product's: "cumin" is what
   *  you look for in a shop, and `product_id` rides along to say precisely which
   *  jar if the line ever named one. Local-first, so it works in the shop. */
  async addMissingToBuy(recipe: Recipe): Promise<void> {
    const missing = this.shoppingFor(recipe.id);
    if (!missing?.length) return;
    const { added, already } = await this.shopping.addMissing(
      missing.map((ing) => ({
        name: ing.name,
        quantity: ing.quantity,
        unit: ing.unit,
        barcode: null,
        category: 'food' satisfies ItemCategory,
        product_id: ing.product_id,
      })),
    );
    this.feedback.notify(this.addedMessage(added.length, already.length));
  }

  /** Says what actually happened, including the nothing-to-do case: a tap that
   *  changed no rows must not read like it added them. */
  private addedMessage(added: number, already: number): string {
    const skipped = already > 0 ? ` (${already} already on it)` : '';
    if (added === 0) return `Already on the Buy list — nothing to add.`;
    return `Added ${added} item${added === 1 ? '' : 's'} to the Buy list${skipped}.`;
  }

  label(ing: RecipeIngredient): string {
    const amount = ing.quantity != null ? `${ing.quantity}${ing.unit ? ' ' + ing.unit : ''} ` : '';
    return `${amount}${ing.name}`;
  }
}
