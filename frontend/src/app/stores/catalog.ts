import { Injectable, inject } from '@angular/core';

import { CachedResource } from '../shared/cached-resource';
import { LifeApi } from '../life-api';
import { BinDay, ConflictEntry, Item, Loc, Recipe, TrashEntry } from '../models';

@Injectable({ providedIn: 'root' })
export class ItemsStore extends CachedResource<Item[]> {
  constructor() {
    const api = inject(LifeApi);
    super(() => api.items());
  }
}

@Injectable({ providedIn: 'root' })
export class BinsStore extends CachedResource<BinDay[]> {
  constructor() {
    const api = inject(LifeApi);
    super(() => api.bins());
  }
}

@Injectable({ providedIn: 'root' })
export class LocationsStore extends CachedResource<Loc[]> {
  constructor() {
    const api = inject(LifeApi);
    super(() => api.locations());
  }
}

@Injectable({ providedIn: 'root' })
export class RecipesStore extends CachedResource<Recipe[]> {
  constructor() {
    const api = inject(LifeApi);
    super(() => api.recipes());
  }
}

@Injectable({ providedIn: 'root' })
export class CookableStore extends CachedResource<Recipe[]> {
  constructor() {
    const api = inject(LifeApi);
    super(() => api.cookable());
  }
}

@Injectable({ providedIn: 'root' })
export class TrashStore extends CachedResource<TrashEntry[]> {
  constructor() {
    const api = inject(LifeApi);
    super(() => api.trash());
  }
}

@Injectable({ providedIn: 'root' })
export class ConflictsStore extends CachedResource<ConflictEntry[]> {
  constructor() {
    const api = inject(LifeApi);
    super(() => api.conflicts());
  }
}

/** "Kitchen › Fridge"; '' when unplaced. Guarded against a parent cycle. */
export function locationPath(byId: Map<number, Loc>, id: number | null): string {
  if (id == null) return '';
  const names: string[] = [];
  const seen = new Set<number>();
  let cur: number | null = id;
  while (cur != null && !seen.has(cur)) {
    seen.add(cur);
    const loc = byId.get(cur);
    if (!loc) break;
    names.unshift(loc.name);
    cur = loc.parent_id;
  }
  return names.join(' › ');
}
