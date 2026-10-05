import { Injectable, inject, signal } from '@angular/core';

import { LifeApi } from '../life-api';

/** Counts shown as menu badges: unresolved sync conflicts. */
@Injectable({ providedIn: 'root' })
export class Alerts {
  private api = inject(LifeApi);

  readonly conflictCount = signal(0);

  refreshConflicts(): void {
    this.api.conflicts().subscribe({
      next: (list) => this.conflictCount.set(list.length),
      error: () => {},
    });
  }

  setConflicts(n: number): void {
    this.conflictCount.set(Math.max(0, n));
  }

  addConflicts(n: number): void {
    if (n > 0) this.conflictCount.update((c) => c + n);
  }
}
