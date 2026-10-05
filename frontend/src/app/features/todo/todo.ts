import { Component, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import { MatBottomSheetModule } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatCheckboxModule } from '@angular/material/checkbox';
import { MatChipsModule } from '@angular/material/chips';
import { MatIconModule } from '@angular/material/icon';
import { MatListModule } from '@angular/material/list';
import { Sheets } from '@xinutec/ui-scaffold';
import { map } from 'rxjs';

import { Feedback } from '../../shared/feedback';
import { ListState } from '../../shared/list-state';
import { TodoPriority, TodoType } from '../../models';
import { TodoDoc, TodoStore } from '../../sync/todo-store';
import { TodoAddSheet } from './todo-add-sheet';
import { TodoDetail } from './todo-detail';
import { DueChip, TodoGraph, TodoState, URGENCY_RANK } from './todo-graph';
import { PRIORITIES, TODO_TYPES, prioRank } from './todo-meta';

@Component({
  selector: 'app-todo',
  templateUrl: './todo.html',
  styleUrl: './todo.scss',
  imports: [
    FormsModule,
    MatListModule,
    MatIconModule,
    MatButtonModule,
    MatCheckboxModule,
    MatChipsModule,
    MatBottomSheetModule,
    ListState,
  ],
})
export class Todo {
  private store = inject(TodoStore);
  private sheet = inject(Sheets);
  private feedback = inject(Feedback);
  readonly graph = inject(TodoGraph);

  constructor() {
    this.graph.refreshCatalogs();
  }

  readonly items = toSignal(this.store.items$, { initialValue: [] as TodoDoc[] });
  /** False until the local DB has produced its first result. */
  readonly loaded = toSignal(this.store.items$.pipe(map(() => true)), { initialValue: false });
  readonly syncError = this.store.syncError;
  readonly types = TODO_TYPES;
  readonly priorities = PRIORITIES;

  openAdd(): void {
    this.sheet.open(TodoAddSheet);
  }

  /** null shows every type. */
  readonly filter = signal<TodoType | null>(null);
  readonly readyOnly = signal(false);
  readonly showWaiting = signal(false);

  /** Everything but waiting: open before done, then urgency, priority, due
   *  date, title. */
  readonly visible = computed(() => {
    const f = this.filter();
    const ready = this.readyOnly();
    return this.items()
      .filter((t) => (f ? t.type === f : true))
      .filter((t) => this.graph.statusOf(t) !== 'waiting')
      .filter((t) => (ready ? this.graph.statusOf(t) === 'ready' : true))
      .slice()
      .sort(this.compare);
  });

  /** Gated by a future start date; hidden under "Ready". */
  readonly waiting = computed(() => {
    if (this.readyOnly()) return [] as TodoDoc[];
    const f = this.filter();
    return this.items()
      .filter((t) => (f ? t.type === f : true))
      .filter((t) => this.graph.statusOf(t) === 'waiting')
      .slice()
      .sort(
        (a, b) =>
          (a.notBefore ?? '').localeCompare(b.notBefore ?? '') || a.title.localeCompare(b.title),
      );
  });
  readonly waitingCount = computed(() => this.waiting().length);

  readonly readyCount = computed(
    () => this.items().filter((t) => this.graph.statusOf(t) === 'ready').length,
  );

  private compare = (a: TodoDoc, b: TodoDoc): number =>
    Number(a.status === 'done') - Number(b.status === 'done') ||
    URGENCY_RANK[this.graph.urgencyOf(a)] - URGENCY_RANK[this.graph.urgencyOf(b)] ||
    prioRank(a.priority) - prioRank(b.priority) ||
    (a.due ?? '9999-99-99').localeCompare(b.due ?? '9999-99-99') ||
    a.title.localeCompare(b.title);

  dueChip(it: TodoDoc): DueChip | null {
    return this.graph.dueChip(it);
  }

  fromLabel(it: TodoDoc): string {
    if (!it.notBefore) return '';
    const d = new Date(it.notBefore + 'T00:00:00');
    return (
      'from ' +
      d.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' })
    );
  }

  setPriority(it: TodoDoc, priority: TodoPriority | null): void {
    void this.store.patch(it.ulid, { priority });
  }

  priorityLabel(p: TodoPriority): string {
    return PRIORITIES.find((x) => x.value === p)?.label ?? p;
  }

  toggle(it: TodoDoc): void {
    // The disabled checkbox's programmatic twin.
    if (it.status !== 'done' && this.graph.statusOf(it) === 'blocked') return;
    void this.store.setStatus(it.ulid, it.status === 'done' ? 'open' : 'done');
  }

  remove(it: TodoDoc): void {
    // Links go only once the Undo window closes, so an undo brings them back too.
    void this.store.remove(it.ulid);
    this.feedback.undo(
      `Deleted “${it.title}”`,
      () => void this.store.undoDelete(it),
      () => this.graph.removeLinksForTodo(it.ulid),
    );
  }

  openDetail(it: TodoDoc): void {
    this.sheet.open(TodoDetail, { data: { ulid: it.ulid } });
  }

  stateOf(it: TodoDoc): TodoState {
    return this.graph.statusOf(it);
  }

  blockerCount(it: TodoDoc): number {
    return this.graph.blockers(it.ulid).length;
  }

  linkCount(it: TodoDoc): number {
    return this.graph.linkCount(it.ulid);
  }

  typeIcon(type: TodoType): string {
    return TODO_TYPES.find((t) => t.value === type)?.icon ?? 'task_alt';
  }

  typeLabel(type: TodoType): string {
    return TODO_TYPES.find((t) => t.value === type)?.label ?? type;
  }
}
