import { TestBed } from '@angular/core/testing';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { Observable, of, throwError } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from '../../shared/feedback';
import { LifeApi } from '../../life-api';
import { Item, NewPurchase, Purchase } from '../../models';
import { PurchaseDialog, PurchaseDialogData } from './purchase-dialog';

const ITEM: Item = {
  id: 93,
  product_id: null,
  name: 'Dishwasher',
  brand: null,
  category: 'appliance',
  quantity: null,
  unit: null,
  expiry: null,
  expiry_precision: 'day',
  location_id: 2,
  barcode: null,
  has_image: false,
};

function setup(
  recordPurchase: (id: number, p: NewPurchase) => Observable<Purchase> = vi.fn(() =>
    of({} as Purchase),
  ),
) {
  const api = { recordPurchase: vi.fn(recordPurchase) };
  const ref = { close: vi.fn() };
  const feedback = { notify: vi.fn(), error: vi.fn() };
  const data: PurchaseDialogData = { item: ITEM };

  TestBed.configureTestingModule({
    imports: [PurchaseDialog],
    providers: [
      { provide: LifeApi, useValue: api },
      { provide: MatDialogRef, useValue: ref },
      { provide: Feedback, useValue: feedback },
      { provide: MAT_DIALOG_DATA, useValue: data },
    ],
  });
  const fixture = TestBed.createComponent(PurchaseDialog);
  fixture.detectChanges();
  return { cmp: fixture.componentInstance, api, ref, feedback };
}

describe('PurchaseDialog', () => {
  it('will not record a purchase of nothing', () => {
    // A shop and a price are the two facts that make this a record rather than a
    // gesture. Everything else is optional and its absence means something.
    const { cmp } = setup();
    expect(cmp.canSave()).toBe(false);
    cmp.shop.set('Currys');
    expect(cmp.canSave()).toBe(false);
    cmp.price.set('349.99');
    expect(cmp.canSave()).toBe(true);
  });

  it('parses money as integer pence, never through a float', () => {
    // 3.30 * 100 is 330.00000000000006. A price that is off by a billionth of a
    // penny is not a price, and the error compounds the moment it is summed.
    const { cmp } = setup();
    cmp.price.set('3.30');
    expect(cmp.pence()).toBe(330);
    cmp.price.set('349.99');
    expect(cmp.pence()).toBe(34999);
    cmp.price.set('not money');
    expect(cmp.pence()).toBeNull();
  });

  it('refuses a warranty that is not whole months, and says which it wants', () => {
    // "2" meaning two YEARS is the mistake a months box invites, and 2.5 is the
    // shape it arrives in. Rounding it to 2 months would store cover nobody was
    // given and look exactly like a real answer.
    const { cmp } = setup();
    cmp.shop.set('Currys');
    cmp.price.set('349.99');
    cmp.warranty.set('2.5');
    expect(cmp.months()).toBeNull();
    expect(cmp.warrantyBad()).toBe(true);
    expect(cmp.canSave()).toBe(false);

    cmp.warranty.set('24');
    expect(cmp.months()).toBe(24);
    expect(cmp.warrantyBad()).toBe(false);
    expect(cmp.canSave()).toBe(true);
  });

  it('treats an empty warranty as no statement, not as a bad one', () => {
    // Most things have no warranty. Blocking the save on an empty box would make
    // the common case the awkward one.
    const { cmp } = setup();
    cmp.shop.set('Currys');
    cmp.price.set('349.99');
    expect(cmp.warrantyBad()).toBe(false);
    expect(cmp.months()).toBeNull();
    expect(cmp.canSave()).toBe(true);
  });

  it('sends nulls, not empty strings, for what was not said', () => {
    // The server reads absent `bought_on` as "now" and absent `warranty_months`
    // as "none recorded". An empty string is neither, and would be a 400.
    const { cmp, api } = setup();
    cmp.shop.set('  Currys  ');
    cmp.price.set('349.99');
    cmp.save();
    expect(api.recordPurchase).toHaveBeenCalledWith(93, {
      shop: 'Currys',
      amount_minor: 34999,
      currency: 'GBP',
      bought_on: null,
      warranty_months: null,
    });
  });

  it('sends the date and the months when they were given', () => {
    const { cmp, api, ref } = setup();
    cmp.shop.set('Currys');
    cmp.price.set('349.99');
    cmp.boughtOn.set('2024-03-15');
    cmp.warranty.set('24');
    cmp.save();
    expect(api.recordPurchase).toHaveBeenCalledWith(93, {
      shop: 'Currys',
      amount_minor: 34999,
      currency: 'GBP',
      bought_on: '2024-03-15',
      warranty_months: 24,
    });
    expect(ref.close).toHaveBeenCalled();
  });

  it('stays open and says so when the save fails', () => {
    // Closing on failure would look exactly like success, and the person would
    // find out by opening the history and seeing nothing.
    const { cmp, ref, feedback } = setup(() => throwError(() => new Error('nope')));
    cmp.shop.set('Currys');
    cmp.price.set('349.99');
    cmp.save();
    expect(ref.close).not.toHaveBeenCalled();
    expect(feedback.error).toHaveBeenCalled();
    expect(cmp.saving()).toBe(false);
  });
});
