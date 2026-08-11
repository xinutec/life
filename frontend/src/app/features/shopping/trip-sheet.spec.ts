import { HttpErrorResponse } from '@angular/common/http';
import { TestBed } from '@angular/core/testing';
import { MAT_BOTTOM_SHEET_DATA, MatBottomSheetRef } from '@angular/material/bottom-sheet';
import { Router } from '@angular/router';
import { of, throwError } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from '../../shared/feedback';
import { LifeApi } from '../../life-api';
import { ShoppingDoc, ShoppingStore } from '../../sync/shopping-store';
import { TripSheet } from './trip-sheet';

const doc = (over: Partial<ShoppingDoc>): ShoppingDoc => ({
  ulid: 'u1',
  id: 1,
  name: 'Yoghurt',
  quantity: null,
  unit: null,
  barcode: null,
  category: 'food',
  product_id: null,
  done: false,
  rev: 1,
  ...over,
});

describe('TripSheet', () => {
  function setup(
    opts: { items?: ShoppingDoc[]; shop?: string; fails?: unknown } = {},
  ) {
    const planShopTrip = vi.fn(() =>
      opts.fails !== undefined
        ? throwError(() => opts.fails)
        : of({ calendar: 'Personal', summary: 'Shop at Asda' }),
    );
    const feedback = { notify: vi.fn(), error: vi.fn(), undo: vi.fn() };
    const ref = { dismiss: vi.fn() };
    const router = { navigate: vi.fn().mockResolvedValue(true) };
    TestBed.configureTestingModule({
      imports: [TripSheet],
      providers: [
        { provide: LifeApi, useValue: { planShopTrip } },
        { provide: ShoppingStore, useValue: { items$: of(opts.items ?? []) } },
        { provide: MatBottomSheetRef, useValue: ref },
        { provide: MAT_BOTTOM_SHEET_DATA, useValue: opts.shop ? { shop: opts.shop } : null },
        { provide: Feedback, useValue: feedback },
        { provide: Router, useValue: router },
      ],
    });
    return {
      fixture: TestBed.createComponent(TripSheet),
      planShopTrip,
      feedback,
      ref,
      router,
    };
  }

  it('opens on the shop the coverage line recommended', () => {
    const { fixture } = setup({ shop: 'Waitrose' });
    expect(fixture.componentInstance.shop()).toBe('Waitrose');
  });

  it('defaults the time to the next whole hour, in local wall-clock', () => {
    const { fixture } = setup();
    const when = fixture.componentInstance.when();
    // The field is local text with no zone in it — an ISO instant here would be
    // an hour out for half the year.
    expect(when).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:00$/);
    expect(new Date(when).getTime()).toBeGreaterThan(Date.now());
  });

  it('sends the un-done rows, so the event lists what is still to get', () => {
    const { fixture, planShopTrip } = setup({
      items: [
        doc({ ulid: 'a', name: 'Milk' }),
        doc({ ulid: 'b', name: 'Bread', done: true }),
        doc({ ulid: 'c', name: 'Eggs' }),
      ],
      shop: 'Asda',
    });
    fixture.componentInstance.save();
    expect(planShopTrip).toHaveBeenCalledWith('Asda', expect.any(String), ['Milk', 'Eggs']);
  });

  it('confirms with the calendar it actually landed in', () => {
    // life picks the calendar, so "added" alone would be unverifiable.
    const { fixture, feedback, ref } = setup({ shop: 'Asda' });
    fixture.componentInstance.save();
    expect(feedback.notify).toHaveBeenCalledWith('“Shop at Asda” added to Personal.');
    expect(ref.dismiss).toHaveBeenCalled();
  });

  it('will not send without a shop', () => {
    const { fixture, planShopTrip } = setup();
    fixture.componentInstance.shop.set('   ');
    expect(fixture.componentInstance.canSave()).toBe(false);
    fixture.componentInstance.save();
    expect(planShopTrip).not.toHaveBeenCalled();
  });

  it('will not send with a half-typed date', () => {
    const { fixture, planShopTrip } = setup({ shop: 'Asda' });
    fixture.componentInstance.when.set('2026-08-');
    expect(fixture.componentInstance.canSave()).toBe(false);
    fixture.componentInstance.save();
    expect(planShopTrip).not.toHaveBeenCalled();
  });

  it('an unlinked calendar stays on screen with the way out of it', () => {
    // 409 is the one failure a retry cannot fix, so it must not be a toast that
    // vanishes while you are still reading it.
    const { fixture, feedback, ref } = setup({
      shop: 'Asda',
      fails: new HttpErrorResponse({ status: 409, error: { error: 'nextcloud not linked' } }),
    });
    fixture.componentInstance.save();
    expect(fixture.componentInstance.needsLinking()).toBe(true);
    expect(feedback.error).not.toHaveBeenCalled();
    expect(ref.dismiss).not.toHaveBeenCalled();
    expect(fixture.componentInstance.saving()).toBe(false);
  });

  it('“Connect it in Settings” closes the sheet and goes there', () => {
    const { fixture, ref, router } = setup({ shop: 'Asda' });
    fixture.componentInstance.openSettings();
    expect(ref.dismiss).toHaveBeenCalled();
    expect(router.navigate).toHaveBeenCalledWith(['/settings']);
  });

  it('offline says so, and keeps the sheet open to try again', () => {
    // withFetch() reports a dropped connection as status 0.
    const { fixture, feedback, ref } = setup({
      shop: 'Asda',
      fails: new HttpErrorResponse({ status: 0 }),
    });
    fixture.componentInstance.save();
    expect(feedback.error).toHaveBeenCalledWith(
      expect.stringContaining('needs a connection'),
    );
    expect(ref.dismiss).not.toHaveBeenCalled();
  });

  it('a server failure is reported rather than swallowed', () => {
    const { fixture, feedback, ref } = setup({
      shop: 'Asda',
      fails: new HttpErrorResponse({ status: 502 }),
    });
    fixture.componentInstance.save();
    expect(feedback.error).toHaveBeenCalledWith('Couldn’t add it to your calendar.');
    expect(ref.dismiss).not.toHaveBeenCalled();
  });
});
