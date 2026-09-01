import { TestBed } from '@angular/core/testing';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { of, throwError } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from '../../shared/feedback';
import { LifeApi } from '../../life-api';
import { Item } from '../../models';
import { UseSheet, UseSheetData } from './use-sheet';

function item(over: Partial<Item> = {}): Item {
  return {
    id: 7,
    product_id: null,
    name: 'Plain flour',
    brand: null,
    category: 'food',
    quantity: 950,
    unit: 'g',
    expiry: null,
    expiry_precision: 'day',
    location_id: null,
    barcode: null,
    has_image: false,
    ...over,
  };
}

function setup(over: Partial<Item> = {}, useItem = vi.fn(() => of(item({ quantity: 750 })))) {
  const api = { useItem };
  const ref = { dismiss: vi.fn() };
  const feedback = { notify: vi.fn(), error: vi.fn() };
  const data: UseSheetData = { item: item(over) };

  TestBed.configureTestingModule({
    imports: [UseSheet],
    providers: [
      { provide: LifeApi, useValue: api },
      { provide: MatBottomSheetRef, useValue: ref },
      { provide: Feedback, useValue: feedback },
      { provide: MAT_BOTTOM_SHEET_DATA, useValue: data },
    ],
  });
  const fixture = TestBed.createComponent(UseSheet);
  fixture.detectChanges();
  return { c: fixture.componentInstance, api, ref, feedback };
}

describe('UseSheet', () => {
  it('offers shares of what is actually on hand', () => {
    // Not fixed amounts: "half the bag" is how you think about it, and half of
    // 950 g is a real amount of a real thing.
    const { c } = setup();
    expect(c.shares()).toEqual([
      { label: '¼', amount: 237.5 },
      { label: '½', amount: 475 },
      { label: 'All of it', amount: 950 },
    ]);
  });

  it('offers no shares when the row tracks no amount', () => {
    // Nothing to take a fraction of; the field still lets you type one.
    const { c } = setup({ quantity: null });
    expect(c.shares()).toEqual([]);
  });

  it('sends the amount in the ITEM\'s unit, never a converted one', () => {
    // The server refuses a mismatch rather than converting, so the sheet must
    // not invent a unit — it sends the row's own.
    const { c, api, ref } = setup();
    c.pick(475);
    c.save();
    expect(api.useItem).toHaveBeenCalledWith(7, 475, 'g');
    expect(ref.dismiss).toHaveBeenCalledWith(true);
  });

  it('says how much is left', () => {
    const { c, feedback } = setup({}, vi.fn(() => of(item({ quantity: 750 }))));
    c.pick(200);
    c.save();
    // Spaced, because an item's unit is free text: "200 g" is the same as
    // "200g" but "1 bottle" is not the same as "1bottle" (see shared/amount).
    expect(feedback.notify).toHaveBeenCalledWith('Used 200 g — 750 g left.');
  });

  it('says it differently when there is none left', () => {
    // "0g left" is technically true and reads like a bug; naming the thing you
    // ran out of is what makes it worth putting back on the Buy list.
    const { c, feedback } = setup({}, vi.fn(() => of(item({ quantity: 0 }))));
    c.pick(950);
    c.save();
    expect(feedback.notify).toHaveBeenCalledWith('Used the last of the plain flour.');
  });

  it('refuses to submit an amount that is not one', () => {
    const { c, api } = setup();
    expect(c.valid()).toBe(false); // nothing typed yet
    c.save();
    c.pick(0);
    c.save();
    c.pick(-5);
    c.save();
    expect(api.useItem).not.toHaveBeenCalled();
  });

  it('keeps the sheet open when the server refuses, so the amount is not lost', () => {
    const { c, ref, feedback } = setup(
      {},
      vi.fn(() => throwError(() => new Error('nope'))),
    );
    c.pick(200);
    c.save();
    expect(feedback.error).toHaveBeenCalled();
    expect(ref.dismiss).not.toHaveBeenCalled();
    expect(c.saving()).toBe(false); // and you can try again
  });
});
