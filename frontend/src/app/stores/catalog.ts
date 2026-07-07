import { Injectable, inject } from '@angular/core';

import { CachedResource } from '../shared/cached-resource';
import { LifeApi } from '../life-api';
import { ConflictEntry, Item, Loc, Recipe, TrashEntry } from '../models';

/** Root-scoped caches for the server read-catalogs that more than one view shows.
 *  Being singletons, they retain their data across a tab switch (the component is
 *  destroyed, the store isn't) and let every view of the same data share one
 *  fetch. Each is just a loader — all the retain/refresh/error logic lives once in
 *  {@link CachedResource}. Call `.refresh()` on entering a view and after a
 *  mutation; read `.value()` / `.loaded()` / `.error()` in the template. */

@Injectable({ providedIn: 'root' })
export class ItemsStore extends CachedResource<Item[]> {
  constructor() {
    const api = inject(LifeApi);
    super(() => api.items());
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
