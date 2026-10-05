import { Component, computed, effect, inject, signal } from '@angular/core';
import { DatePipe } from '@angular/common';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { scaffoldTitle } from '@xinutec/ui-scaffold';

import { Alerts } from '../../shared/alerts';
import { Feedback } from '../../shared/feedback';
import { ListState } from '../../shared/list-state';
import { LifeApi } from '../../life-api';
import { ConflictsStore } from '../../stores/catalog';
import { ConflictEntry } from '../../models';
import { SHOPPING_MERGE_FIELDS, ShoppingStore } from '../../sync/shopping-store';
import { TODO_MERGE_FIELDS, TodoStore } from '../../sync/todo-store';
import { WELLBEING_MERGE_FIELDS, WellbeingStore } from '../../sync/wellbeing-store';

const SHOPPING_PATCHABLE: ReadonlySet<string> = new Set(SHOPPING_MERGE_FIELDS);
const TODO_PATCHABLE: ReadonlySet<string> = new Set(TODO_MERGE_FIELDS);
const WELLBEING_PATCHABLE: ReadonlySet<string> = new Set(WELLBEING_MERGE_FIELDS);

/** Values the merge discarded when two devices edited one field: keep the
 *  chosen one, or switch. */
@Component({
  selector: 'app-conflicts',
  templateUrl: './conflicts.html',
  styleUrl: './conflicts.scss',
  imports: [DatePipe, MatButtonModule, MatCardModule, ListState],
})
export class Conflicts {
  private api = inject(LifeApi);
  private feedback = inject(Feedback);
  private shopping = inject(ShoppingStore);
  private todo = inject(TodoStore);
  private wellbeing = inject(WellbeingStore);
  private alerts = inject(Alerts);
  private conflictsStore = inject(ConflictsStore);

  readonly entries = computed(() => this.conflictsStore.value() ?? []);
  readonly loaded = this.conflictsStore.loaded;
  readonly error = this.conflictsStore.error;
  readonly refreshing = this.conflictsStore.refreshing;
  readonly busy = signal<ReadonlySet<number>>(new Set());

  constructor() {
    scaffoldTitle(() => 'Sync conflicts');
    this.conflictsStore.refresh();
    effect(() => {
      if (this.loaded()) this.alerts.setConflicts(this.entries().length);
    });
  }

  reload(): void {
    this.conflictsStore.refresh();
  }

  fmt(encoded: string): string {
    try {
      const v: unknown = JSON.parse(encoded);
      if (v === null || v === '') return '(empty)';
      if (Array.isArray(v)) return v.length ? v.join(', ') : '(none)';
      if (typeof v === 'boolean') return v ? 'yes' : 'no';
      if (typeof v === 'string' || typeof v === 'number') return `${v}`;
      return encoded; // scalars + lists handled; anything else shows raw
    } catch {
      return encoded;
    }
  }

  keepMine(e: ConflictEntry): void {
    this.finish(e, undefined);
  }

  useTheirs(e: ConflictEntry): void {
    const value: unknown = JSON.parse(e.theirs);
    // Checked against the store's patchable fields before writing.
    let apply: Promise<void> | undefined;
    if (e.kind === 'shopping' && SHOPPING_PATCHABLE.has(e.field)) {
      apply = this.shopping.patch(e.ulid, { [e.field]: value });
    } else if (e.kind === 'todo' && TODO_PATCHABLE.has(e.field)) {
      apply = this.todo.patch(e.ulid, { [e.field]: value });
    } else if (e.kind === 'wellbeing' && WELLBEING_PATCHABLE.has(e.field)) {
      apply = this.wellbeing.patch(e.ulid, { [e.field]: value });
    }
    if (!apply) {
      this.feedback.error('This conflict can no longer be applied.');
      return;
    }
    this.finish(e, apply);
  }

  private finish(e: ConflictEntry, apply: Promise<void> | undefined): void {
    this.busy.update((s) => new Set(s).add(e.id));
    void (apply ?? Promise.resolve()).then(() => {
      this.api.resolveConflict(e.id).subscribe({
        next: () => {
          this.busy.update((s) => {
            const next = new Set(s);
            next.delete(e.id);
            return next;
          });
          this.conflictsStore.patch((list) => (list ?? []).filter((x) => x.id !== e.id));
        },
        error: () => {
          this.busy.update((s) => {
            const next = new Set(s);
            next.delete(e.id);
            return next;
          });
          this.feedback.error('Could not resolve — are you online?');
        },
      });
    });
  }
}
