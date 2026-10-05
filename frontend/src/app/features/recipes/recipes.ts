import { Component, computed, inject, signal } from '@angular/core';
import { MatBottomSheetModule } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatChipsModule } from '@angular/material/chips';
import { MatIconModule } from '@angular/material/icon';
import { Sheets } from '@xinutec/ui-scaffold';

import { amount } from '../../shared/amount';
import { onlineHint } from '../../shared/api-error';
import { Feedback } from '../../shared/feedback';
import { ListState } from '../../shared/list-state';
import { LifeApi } from '../../life-api';
import { CookableStore, RecipesStore } from '../../stores/catalog';
import { CookedLine, ItemCategory, Recipe, RecipeIngredient } from '../../models';
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
  private sheet = inject(Sheets);
  private feedback = inject(Feedback);
  private recipesStore = inject(RecipesStore);
  private cookableStore = inject(CookableStore);
  private shopping = inject(ShoppingStore);

  private failed(what: string) {
    return (e: unknown) => {
      this.feedback.error(`Could not ${what}${onlineHint(e)}`);
    };
  }

  readonly recipes = computed(() => this.recipesStore.value() ?? []);
  readonly loaded = this.recipesStore.loaded;
  readonly loadError = this.recipesStore.error;
  readonly refreshing = this.recipesStore.refreshing;
  readonly cookableIds = computed(
    () => new Set((this.cookableStore.value() ?? []).map((r) => r.id)),
  );
  /** Per recipe, what is short; loaded on demand. */
  private readonly missingByRecipe = signal<Map<number, RecipeIngredient[]>>(new Map());

  /** The last cook's report per recipe, until you leave the screen. */
  private readonly cookedByRecipe = signal<Map<number, CookedLine[]>>(new Map());
  private readonly cooking = signal<number | null>(null);

  readonly cookableCount = computed(() => this.cookableIds().size);

  addRecipe(): void {
    this.openSheet();
  }

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

  /** Shown in full: most lines cannot be settled ("salt", a jar against grams),
   *  and a report of successes only would overstate what changed. */
  cookIt(recipe: Recipe): void {
    if (this.cooking() !== null) return;
    this.cooking.set(recipe.id);
    this.api.cookRecipe(recipe.id).subscribe({
      next: (lines) => {
        this.cooking.set(null);
        const next = new Map(this.cookedByRecipe());
        next.set(recipe.id, lines);
        this.cookedByRecipe.set(next);
        const took = lines.filter((l) => l.kind !== 'untouched').length;
        this.feedback.notify(
          took > 0
            ? `Took ${took} of ${lines.length} off the shelf.`
            : `Nothing came off the shelf — see why below.`,
        );
        this.cookableStore.refresh();
      },
      error: (e: unknown) => {
        this.cooking.set(null);
        this.feedback.error(`Could not record cooking that${onlineHint(e)}`);
      },
    });
  }

  cookedFor(id: number): CookedLine[] | undefined {
    return this.cookedByRecipe().get(id);
  }

  isCooking(id: number): boolean {
    return this.cooking() === id;
  }

  /** Exhaustive, so a new outcome is a compile error, not a blank row. */
  cookedLabel(line: CookedLine): string {
    switch (line.kind) {
      case 'took':
        return line.from.map((t) => `${amount(t.amount, line.unit)} from ${t.name}`).join(', ');
      case 'short':
        return line.from.length
          ? `used what there was, ${amount(line.short, line.unit)} short`
          : `nothing there, ${amount(line.short, line.unit)} short`;
      case 'untouched':
        switch (line.why) {
          case 'no_stock':
            return "you don't have any";
          case 'no_amount':
            return 'the recipe gives no amount';
          case 'no_comparable_stock':
            return 'the cupboard measures it differently';
        }
    }
  }

  /** What the cupboard lacks, onto the Buy list with the amount short. */
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

  /** A tap that added nothing must not read as if it had. */
  private addedMessage(added: number, already: number): string {
    const skipped = already > 0 ? ` (${already} already on it)` : '';
    if (added === 0) return `Already on the Buy list — nothing to add.`;
    return `Added ${added} item${added === 1 ? '' : 's'} to the Buy list${skipped}.`;
  }

  label(ing: RecipeIngredient): string {
    return [amount(ing.quantity, ing.unit), ing.name].filter((s) => s).join(' ');
  }
}
