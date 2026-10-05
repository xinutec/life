import { Component, computed, inject, signal } from '@angular/core';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatListModule } from '@angular/material/list';

import { ago } from '../../shared/ago';
import { onlineHint } from '../../shared/api-error';
import { Dialog } from '../../shared/dialog';
import { Feedback } from '../../shared/feedback';
import { ListState } from '../../shared/list-state';
import { LifeApi } from '../../life-api';
import { Item, ItemFile } from '../../models';

export interface FilesDialogData {
  item: Item;
}

/** 10 MiB, as the server: refused before the upload, not after. */
const MAX_BYTES = 10 * 1024 * 1024;

/** Receipts and manuals, on the item: a receipt is personal, and a hand-entered
 *  appliance has no product. */
@Component({
  selector: 'app-files-dialog',
  templateUrl: './files-dialog.html',
  styleUrl: './files-dialog.scss',
  imports: [Dialog, ListState, MatButtonModule, MatIconModule, MatListModule],
})
export class FilesDialog {
  private ref = inject(MatDialogRef<FilesDialog, void>);
  private data = inject<FilesDialogData>(MAT_DIALOG_DATA);
  private api = inject(LifeApi);
  private feedback = inject(Feedback);

  readonly item = this.data.item;
  readonly files = signal<ItemFile[] | null>(null);
  readonly error = signal<string | null>(null);
  readonly uploading = signal(false);

  readonly loaded = computed(() => this.files() !== null);

  readonly rows = computed(() =>
    (this.files() ?? []).map((f) => ({
      id: f.id,
      ...split(f.name),
      href: this.api.fileUrl(this.item.id, f.id),
      icon: f.mime === 'application/pdf' ? 'picture_as_pdf' : 'image',
      detail: [size(f.size_bytes), ago(f.created_at)].join(' · '),
    })),
  );

  constructor() {
    this.load();
  }

  load(): void {
    this.files.set(null);
    this.error.set(null);
    this.api.itemFiles(this.item.id).subscribe({
      next: (f) => this.files.set(f),
      error: (e: unknown) => this.error.set(`Could not read the files${onlineHint(e)}`),
    });
  }

  /** Cleared first, so the same file can be picked again. No `capture`, which
   *  would hide the photo library. */
  pick(event: Event): void {
    const input = event.target;
    if (!(input instanceof HTMLInputElement)) return;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    if (file.size === 0) {
      this.feedback.error('That file is empty.');
      return;
    }
    if (file.size > MAX_BYTES) {
      this.feedback.error('That file is bigger than 10 MB.');
      return;
    }
    this.uploading.set(true);
    this.api.addItemFile(this.item.id, file).subscribe({
      next: () => {
        this.uploading.set(false);
        this.load();
      },
      error: (e: unknown) => {
        this.uploading.set(false);
        this.feedback.error(`Could not attach it${onlineHint(e)}`);
      },
    });
  }

  remove(id: number): void {
    this.api.deleteItemFile(this.item.id, id).subscribe({
      next: () => {
        this.load();
        this.feedback.undo('File removed', () => {
          this.api.restoreTrash('file', String(id)).subscribe({
            next: () => this.load(),
            error: (e: unknown) => this.feedback.error(`Could not undo${onlineHint(e)}`),
          });
        });
      },
      error: (e: unknown) => this.feedback.error(`Could not remove it${onlineHint(e)}`),
    });
  }

  close(): void {
    this.ref.close();
  }
}

/** A name split so its end survives truncation: phones name scans `IMG_2024…`. */
function split(name: string): { head: string; tail: string; full: string } {
  const keep = Math.min(12, name.length);
  return {
    head: name.slice(0, name.length - keep),
    tail: name.slice(name.length - keep),
    full: name,
  };
}

/** "1.4 MB", "640 KB". */
function size(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}
