import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { MatBottomSheet } from '@angular/material/bottom-sheet';
import { of } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from '../../shared/feedback';
import { LifeApi } from '../../life-api';
import { Recipe, RecipeIngredient } from '../../models';
import { CookableStore, RecipesStore } from '../../stores/catalog';
import { ShoppingStore } from '../../sync/shopping-store';
import { Recipes } from './recipes';

const ing = (over: Partial<RecipeIngredient>): RecipeIngredient => ({
  name: 'Cumin',
  product_id: null,
  product_name: null,
  quantity: null,
  unit: null,
  ...over,
});

const recipe: Recipe = {
  id: 3,
  name: 'Dal',
  instructions: null,
  servings: null,
  ingredients: [ing({ name: 'cumin' })],
};

/** A cached-root-store stub: the four signals the template reads plus refresh. */
const catalog = <T>(value: T) => ({
  value: signal(value),
  loaded: signal(true),
  error: signal<string | null>(null),
  refreshing: signal(false),
  refresh: vi.fn(),
});

function setup(missing: RecipeIngredient[], already: string[] = []) {
  const shopping = {
    addMissing: vi.fn((inputs: { name: string }[]) =>
      Promise.resolve({
        added: inputs.filter((i) => !already.includes(i.name)).map((i) => i.name),
        already,
      }),
    ),
  };
  const api = { shoppingList: vi.fn(() => of(missing)) };
  const feedback = { notify: vi.fn(), error: vi.fn(), undo: vi.fn() };
  TestBed.configureTestingModule({
    providers: [
      Recipes,
      { provide: RecipesStore, useValue: catalog([recipe]) },
      { provide: CookableStore, useValue: catalog([]) },
      { provide: ShoppingStore, useValue: shopping },
      { provide: LifeApi, useValue: api },
      { provide: Feedback, useValue: feedback },
      { provide: MatBottomSheet, useValue: { open: vi.fn() } },
    ],
  });
  return { c: TestBed.inject(Recipes), shopping, api, feedback };
}

describe('Recipe→Buy bridge', () => {
  it('sends what the recipe is short of, quantity and link included', async () => {
    const { c, shopping } = setup([ing({ name: 'cumin', quantity: 2, unit: 'tsp', product_id: 42 })]);
    c.loadShoppingList(3);
    await c.addMissingToBuy(recipe);
    expect(shopping.addMissing).toHaveBeenCalledWith([
      { name: 'cumin', quantity: 2, unit: 'tsp', barcode: null, category: 'food', product_id: 42 },
    ]);
  });

  it('does nothing at all before the shopping list has been loaded', () => {
    // The list is loaded on demand; without it we don't know what's missing,
    // and adding every ingredient would put the whole cupboard on the list.
    const { c, shopping } = setup([ing({})]);
    void c.addMissingToBuy(recipe);
    expect(shopping.addMissing).not.toHaveBeenCalled();
  });

  it('says how many it added', async () => {
    const { c, feedback } = setup([ing({ name: 'cumin' }), ing({ name: 'rice' })]);
    c.loadShoppingList(3);
    await c.addMissingToBuy(recipe);
    expect(feedback.notify).toHaveBeenCalledWith('Added 2 items to the Buy list.');
  });

  it('counts the ones that were already there separately', async () => {
    const { c, feedback } = setup([ing({ name: 'cumin' }), ing({ name: 'rice' })], ['rice']);
    c.loadShoppingList(3);
    await c.addMissingToBuy(recipe);
    expect(feedback.notify).toHaveBeenCalledWith('Added 1 item to the Buy list (1 already on it).');
  });

  it('does not claim to have added anything when it added nothing', async () => {
    const { c, feedback } = setup([ing({ name: 'cumin' })], ['cumin']);
    c.loadShoppingList(3);
    await c.addMissingToBuy(recipe);
    expect(feedback.notify).toHaveBeenCalledWith('Already on the Buy list — nothing to add.');
  });
});
