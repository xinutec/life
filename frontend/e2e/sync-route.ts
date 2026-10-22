import type { Page } from '@playwright/test';

import type { PullResponse } from '../src/app/models';
import type { Synced } from '../src/app/sync/replication';
import type { ShoppingDoc } from '../src/app/sync/shopping-store';
import type { TodoDoc } from '../src/app/sync/todo-store';
import type { TodoLinkDoc } from '../src/app/sync/todo-link-store';
import type { WellbeingDoc } from '../src/app/sync/wellbeing-store';

export type SyncDoc =
  Synced<TodoDoc> | Synced<TodoLinkDoc> | Synced<ShoppingDoc> | Synced<WellbeingDoc>;

/** The incremental sync pull, as a route handler; a push answers no conflicts. */
export const syncRoute = (docs: SyncDoc[]) => (r: Parameters<Parameters<Page['route']>[1]>[0]) => {
  if (r.request().method() === 'POST') return r.fulfill({ json: [] satisfies SyncDoc[] });
  const since = Number(new URL(r.request().url()).searchParams.get('since') ?? '0');
  // Only rows past the checkpoint, or the pull re-sends the seed forever.
  const fresh = docs.filter((d) => d.rev > since);
  const top = docs.reduce((m, d) => Math.max(m, d.rev), since);
  return r.fulfill({
    json: { documents: fresh, checkpoint: { rev: top } } satisfies PullResponse<SyncDoc>,
  });
};
