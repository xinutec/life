import { TestBed } from '@angular/core/testing';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { MatDialog } from '@angular/material/dialog';
import { of } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from '../../shared/feedback';
import { ProductPick } from '../../shared/product-picker';
import { LifeApi } from '../../life-api';
import { RecipeSheet, RecipeSheetData } from './recipe-sheet';

const flush = () => new Promise((r) => setTimeout(r));

function setup(opts: { pick?: ProductPick | null; data?: RecipeSheetData } = {}) {
  const dialog = { open: vi.fn(() => ({ afterClosed: () => of(opts.pick ?? null) })) };
  const api = {
    createRecipe: vi.fn(() => of({ id: 3 })),
    updateRecipe: vi.fn(() => of({ id: 3 })),
  };
  const ref = { dismiss: vi.fn() };
  const feedback = { notify: vi.fn(), error: vi.fn() };

  TestBed.configureTestingModule({
    imports: [RecipeSheet],
    providers: [
      { provide: MatDialog, useValue: dialog },
      { provide: LifeApi, useValue: api },
      { provide: MatBottomSheetRef, useValue: ref },
      { provide: Feedback, useValue: feedback },
      { provide: MAT_BOTTOM_SHEET_DATA, useValue: opts.data ?? null },
    ],
  });
  // The sheet imports MatDialogModule, which re-provides the real MatDialog at
  // the component injector — overrideProvider forces our stub at every level.
  TestBed.overrideProvider(MatDialog, { useValue: dialog });
  const fixture = TestBed.createComponent(RecipeSheet);
  fixture.detectChanges();
  return { cmp: fixture.componentInstance, dialog, api, ref, feedback };
}

const catalogPick: ProductPick = {
  name: 'Bart Ground Cumin 38g',
  barcode: '5099999900040',
  product_id: 42,
  quantity: null,
  unit: 'g',
  category: 'food',
};

describe('RecipeSheet product linking', () => {
  it('keeps the line called what the cook calls it', async () => {
    // The whole point of the link is that the two names DIFFER — the recipe
    // says "cumin" and the shop says "Bart Ground Cumin 38g". Overwriting the
    // line with the shop's name would make the recipe unreadable to fix a
    // matching problem the link already fixes.
    const { cmp } = setup({ pick: catalogPick });
    cmp.patchIngredient(0, { name: 'cumin' });
    cmp.linkProduct(0);
    await flush();
    expect(cmp.form().ingredients[0].name).toBe('cumin');
    expect(cmp.form().ingredients[0].product_id).toBe(42);
    expect(cmp.form().ingredients[0].product_name).toBe('Bart Ground Cumin 38g');
  });

  it('names a blank line after the product, since the line has no name of its own', async () => {
    const { cmp } = setup({ pick: catalogPick });
    cmp.linkProduct(0);
    await flush();
    expect(cmp.form().ingredients[0].name).toBe('Bart Ground Cumin 38g');
  });

  it('takes the unit only when the line has none', async () => {
    const { cmp } = setup({ pick: catalogPick });
    cmp.patchIngredient(0, { name: 'cumin', unit: 'tsp' });
    cmp.linkProduct(0);
    await flush();
    expect(cmp.form().ingredients[0].unit).toBe('tsp');
  });

  it('takes the linked product’s unit but never its pack size', async () => {
    // A 38g jar of cumin is what the shop sells; it is not how much cumin the
    // recipe wants. The unit IS worth taking — it is how a line gets one
    // without anyone typing it, and comparable stock depends on having one.
    const { cmp } = setup({ pick: { ...catalogPick, quantity: 38, unit: 'g' } });
    cmp.patchIngredient(0, { name: 'cumin' });
    cmp.linkProduct(0);
    await flush();
    expect(cmp.form().ingredients[0].unit).toBe('g');
    expect(cmp.form().ingredients[0].quantity).toBeNull();
  });

  it('says so when the pick has no product behind it, instead of looking linked', async () => {
    // The picker's inventory tier can hand back an item you typed in yourself,
    // which has no catalog product. A row left claiming a link it doesn't have
    // would match by name while showing the link icon.
    const { cmp, feedback } = setup({
      pick: {
        name: 'Homemade stock',
        barcode: null,
        product_id: null,
        quantity: null,
        unit: null,
        category: null,
      },
    });
    cmp.linkProduct(0);
    await flush();
    expect(cmp.form().ingredients[0].product_id).toBeNull();
    expect(cmp.form().ingredients[0].product_name).toBeNull();
    expect(feedback.error).toHaveBeenCalled();
  });

  it('cancelling the picker changes nothing', async () => {
    const { cmp } = setup({ pick: null });
    cmp.patchIngredient(0, { name: 'cumin' });
    cmp.linkProduct(0);
    await flush();
    expect(cmp.form().ingredients[0]).toEqual({
      name: 'cumin',
      product_id: null,
      product_name: null,
      quantity: null,
      unit: null,
    });
  });

  it('unlinking drops the link but keeps the line', async () => {
    const { cmp } = setup({ pick: catalogPick });
    cmp.patchIngredient(0, { name: 'cumin' });
    cmp.linkProduct(0);
    await flush();
    cmp.unlinkProduct(0);
    expect(cmp.form().ingredients[0].name).toBe('cumin');
    expect(cmp.form().ingredients[0].product_id).toBeNull();
    expect(cmp.form().ingredients[0].product_name).toBeNull();
  });

  it('sends the link on save', async () => {
    const { cmp, api } = setup({ pick: catalogPick });
    cmp.patch({ name: 'Dal' });
    cmp.patchIngredient(0, { name: 'cumin' });
    cmp.linkProduct(0);
    await flush();
    cmp.save();
    expect(api.createRecipe).toHaveBeenCalledWith(
      expect.objectContaining({
        ingredients: [expect.objectContaining({ name: 'cumin', product_id: 42 })],
      }),
    );
  });
});
