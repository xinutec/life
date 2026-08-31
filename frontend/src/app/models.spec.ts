import { describe, expect, it } from 'vitest';

import { ITEM_CATEGORIES, ITEM_CATEGORY_LABEL } from './models';

describe('item categories', () => {
  it('names every category it offers', () => {
    // The list and the labels are two Records over the same union, so TypeScript
    // already catches a missing entry. This catches the runtime half: a label
    // that is empty or still the slug would render as a bug in the data rather
    // than as a category.
    for (const c of ITEM_CATEGORIES) {
      const label = ITEM_CATEGORY_LABEL[c];
      expect(label, c).toBeTruthy();
      expect(label, c).not.toBe(c);
    }
  });

  it('offers "other" last', () => {
    // A fallback offered early gets picked early — which is how `other` came to
    // hold an avocado and a protein drink alongside the pans.
    expect(ITEM_CATEGORIES.at(-1)).toBe('other');
  });

  it('has no two categories reading the same', () => {
    const labels = ITEM_CATEGORIES.map((c) => ITEM_CATEGORY_LABEL[c]);
    expect(new Set(labels).size).toBe(labels.length);
  });
});
