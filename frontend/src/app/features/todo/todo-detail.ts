import { Component, OnDestroy, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatBottomSheetRef, MAT_BOTTOM_SHEET_DATA } from '@angular/material/bottom-sheet';
import { MatButtonModule } from '@angular/material/button';
import { MatCheckboxModule } from '@angular/material/checkbox';
import { MatChipsModule } from '@angular/material/chips';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatListModule } from '@angular/material/list';
import { MatSlideToggleModule } from '@angular/material/slide-toggle';
import { Sheets } from '@xinutec/ui-scaffold';

import { localDay } from '../../shared/civil-day';
import { Feedback } from '../../shared/feedback';
import { LinkKind, TodoPriority, TodoType } from '../../models';
import { TodoStore } from '../../sync/todo-store';
import { LinkTarget, TodoGraph } from './todo-graph';
import { PRIORITIES, TODO_TYPES } from './todo-meta';

const KINDS: readonly { value: LinkKind; label: string }[] = [
  { value: 'depends_on', label: 'Depends on' },
  { value: 'subtask', label: 'Subtask' },
  { value: 'related', label: 'Related' },
];

function presetDate(kind: 'today' | 'tomorrow' | 'weekend' | 'nextweek'): string {
  const d = new Date();
  if (kind === 'tomorrow') d.setDate(d.getDate() + 1);
  else if (kind === 'weekend')
    d.setDate(d.getDate() + ((6 - d.getDay() + 7) % 7)); // next Sat
  else if (kind === 'nextweek') d.setDate(d.getDate() + ((1 - d.getDay() + 7) % 7 || 7)); // next Mon
  return localDay(d);
}

/** One heading's connections; `todoRef` is set when the target is a to-do. */
interface Group {
  heading: string;
  rows: { edge: string; target: LinkTarget; todoRef: string | null }[];
}

@Component({
  selector: 'app-todo-detail',
  templateUrl: './todo-detail.html',
  styleUrl: './todo-detail.scss',
  imports: [
    FormsModule,
    MatButtonModule,
    MatCheckboxModule,
    MatChipsModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatListModule,
    MatSlideToggleModule,
  ],
})
export class TodoDetail implements OnDestroy {
  private deleting = false;
  // Only typed fields are flushed on close, so a remote edit made while the
  // sheet was open is not overwritten by the stale original.
  private titleDirty = false;
  private notesDirty = false;

  // A dismiss may skip the blur that saves, so flush what was typed.
  ngOnDestroy(): void {
    if (this.deleting) return;
    const t = this.todo();
    if (!t) return;
    if (this.titleDirty) {
      const title = this.title().trim();
      if (title && title !== t.title) void this.store.patch(this.ulid(), { title });
    }
    if (this.notesDirty) {
      const notes = this.notes().trim() || null;
      if (notes !== (t.notes ?? null)) void this.store.patch(this.ulid(), { notes });
    }
  }

  private ref = inject(MatBottomSheetRef<TodoDetail>);
  private data = inject<{ ulid: string }>(MAT_BOTTOM_SHEET_DATA);
  private store = inject(TodoStore);
  private sheet = inject(Sheets);
  private feedback = inject(Feedback);
  readonly graph = inject(TodoGraph);

  constructor() {
    this.graph.refreshCatalogs();
  }

  readonly types = TODO_TYPES;
  readonly kinds = KINDS;
  readonly priorities = PRIORITIES;
  readonly ulid = signal(this.data.ulid);

  readonly todo = computed(() => this.graph.todoItems().find((t) => t.ulid === this.ulid()));
  readonly state = computed(() => {
    const t = this.todo();
    return t ? this.graph.statusOf(t) : 'open';
  });
  readonly blockers = computed(() => this.graph.blockers(this.ulid()));

  readonly groups = computed<Group[]>(() => {
    const out = this.graph.outgoing(this.ulid());
    const inc = this.graph.incoming(this.ulid());
    const toTodoRef = (t: LinkTarget) => (t.kind === 'todo' ? t.ref : null);
    const g: Group[] = [
      {
        heading: 'Depends on',
        rows: out
          .filter((l) => l.linkKind === 'depends_on')
          .map((l) => ({ edge: l.ulid, target: l.target, todoRef: toTodoRef(l.target) })),
      },
      {
        heading: 'Needed by',
        rows: inc
          .filter((l) => l.linkKind === 'depends_on')
          .map((l) => ({ edge: l.ulid, target: l.source, todoRef: l.source.ref })),
      },
      {
        heading: 'Subtasks',
        rows: out
          .filter((l) => l.linkKind === 'subtask')
          .map((l) => ({ edge: l.ulid, target: l.target, todoRef: toTodoRef(l.target) })),
      },
      {
        heading: 'Part of',
        rows: inc
          .filter((l) => l.linkKind === 'subtask')
          .map((l) => ({ edge: l.ulid, target: l.source, todoRef: l.source.ref })),
      },
      {
        heading: 'Related',
        rows: [
          ...out
            .filter((l) => l.linkKind === 'related')
            .map((l) => ({ edge: l.ulid, target: l.target, todoRef: toTodoRef(l.target) })),
          ...inc
            .filter((l) => l.linkKind === 'related')
            .map((l) => ({ edge: l.ulid, target: l.source, todoRef: l.source.ref })),
        ],
      },
    ];
    return g.filter((grp) => grp.rows.length > 0);
  });

  readonly title = signal(this.todo()?.title ?? '');
  readonly notes = signal(this.todo()?.notes ?? '');

  readonly addKind = signal<LinkKind>('related');
  readonly query = signal('');
  readonly results = computed(() => this.graph.search(this.query(), this.ulid()));

  /** Marks the field typed, for the flush on close. */
  onTitleInput(value: string): void {
    this.titleDirty = true;
    this.title.set(value);
  }

  onNotesInput(value: string): void {
    this.notesDirty = true;
    this.notes.set(value);
  }

  saveTitle(): void {
    this.titleDirty = false;
    const t = this.title().trim();
    if (t) void this.store.patch(this.ulid(), { title: t });
  }

  saveNotes(): void {
    this.notesDirty = false;
    void this.store.patch(this.ulid(), { notes: this.notes().trim() || null });
  }

  // A chip listbox emits undefined on deselect.
  setType(type: TodoType | undefined): void {
    if (type == null) return;
    void this.store.patch(this.ulid(), { type });
  }

  setShared(shared: boolean): void {
    void this.store.patch(this.ulid(), { shared });
  }

  setPriority(priority: TodoPriority | null | undefined): void {
    if (priority === undefined) return;
    void this.store.patch(this.ulid(), { priority });
  }

  setAddKind(kind: LinkKind | undefined): void {
    if (kind == null) return;
    this.addKind.set(kind);
  }

  readonly datePresets = [
    { label: 'Today', kind: 'today' },
    { label: 'Tomorrow', kind: 'tomorrow' },
    { label: 'Weekend', kind: 'weekend' },
    { label: 'Next week', kind: 'nextweek' },
  ] as const;

  // A cleared date input emits ''.
  private clean(v: string | null): string | null {
    return v && v.trim().length > 0 ? v : null;
  }

  setNotBefore(v: string | null): void {
    void this.store.patch(this.ulid(), { notBefore: this.clean(v) });
  }

  setDue(v: string | null): void {
    void this.store.patch(this.ulid(), { due: this.clean(v) });
  }

  applyPreset(
    field: 'notBefore' | 'due',
    kind: 'today' | 'tomorrow' | 'weekend' | 'nextweek',
  ): void {
    const iso = presetDate(kind);
    if (field === 'notBefore') this.setNotBefore(iso);
    else this.setDue(iso);
  }

  toggleDone(): void {
    const t = this.todo();
    if (!t) return;
    if (t.status !== 'done' && this.state() === 'blocked') return;
    void this.store.setStatus(this.ulid(), t.status === 'done' ? 'open' : 'done');
  }

  addLink(target: LinkTarget): void {
    this.graph.add({
      from: this.ulid(),
      kind: this.addKind(),
      targetKind: target.kind,
      targetRef: target.ref,
    });
    this.query.set('');
  }

  removeLink(edge: string): void {
    this.graph.removeLink(edge);
  }

  openTodo(ref: string): void {
    this.ref.dismiss();
    this.sheet.open(TodoDetail, { data: { ulid: ref } });
  }

  remove(): void {
    const key = this.ulid();
    const doc = this.todo();
    this.deleting = true; // don't let ngOnDestroy re-save the row we're removing
    void this.store.remove(key);
    this.ref.dismiss();
    if (!doc) return;
    // Links go only once the Undo window closes, so an undo brings them back too.
    this.feedback.undo(
      `Deleted “${doc.title}”`,
      () => void this.store.undoDelete(doc),
      () => this.graph.removeLinksForTodo(key),
    );
  }

  close(): void {
    this.ref.dismiss();
  }
}
