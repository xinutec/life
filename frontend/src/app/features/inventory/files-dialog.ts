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

/** 10 MiB, matching the server. Checked here too so a too-big file is refused
 *  before it is uploaded rather than after — the round trip is the expensive
 *  part on a phone. */
const MAX_BYTES = 10 * 1024 * 1024;

/**
 * Receipts and manuals for one item.
 *
 * The catalogue stores an image per PRODUCT, keyed on a barcode, which could not
 * hold any of this: products are shared reference data and a receipt is
 * personal, and an appliance entered by hand has neither a barcode nor a
 * product. So files hang off the item.
 *
 * A dialog over the item sheet, like History and the purchase form, because
 * `MatBottomSheet` holds one sheet at a time and a second would dismiss the
 * edit form under it.
 */
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
      // The icon says what it is faster than the mime string would, and the
      // mime is not something anybody wants to read.
      icon: f.mime === 'application/pdf' ? 'picture_as_pdf' : 'image',
      detail: [size(f.size_bytes), ago(new Date(f.created_at).getTime())].join(' · '),
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

  /** The hidden input's change. Cleared before reading so the same file can be
   *  picked twice after a failure, and no `capture` attribute: that would force
   *  the camera and remove the photo library, which is where a scanned manual
   *  lives. */
  pick(event: Event): void {
    // A guard, not a cast: `event.target` is EventTarget and narrowing it by
    // assertion is a claim the compiler cannot check. Same shape as
    // `product.ts`'s `pickImage`, which says so in the same words.
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
      next: () => this.load(),
      error: (e: unknown) => this.feedback.error(`Could not remove it${onlineHint(e)}`),
    });
  }

  close(): void {
    this.ref.close();
  }
}

/**
 * A filename split so the END of it always survives.
 *
 * A plain CSS ellipsis cuts the tail, and the tail is where the meaning is: a
 * phone names a scan `IMG_20240315_143022_receipt_dishwasher.pdf` by itself, so
 * trimming the right leaves a column of identical `IMG_2024…` rows with nothing
 * to tell the receipt from the manual.
 *
 * ⚠ Two spans, not a character count. The first attempt trimmed to 34
 * characters and it was still clipped: the row is 192 px and 34 characters
 * rendered at 307, so it produced TWO ellipses and cut the extension anyway.
 * A character budget cannot be right — glyphs are different widths and the row
 * changes with the viewport. So the head shrinks under CSS and the tail is
 * `flex: 0 0 auto`, which is correct at every width by construction.
 */
function split(name: string): { head: string; tail: string; full: string } {
  const keep = Math.min(12, name.length);
  return {
    head: name.slice(0, name.length - keep),
    tail: name.slice(name.length - keep),
    full: name,
  };
}

/** Bytes as somebody would say them. One decimal for MB, none for KB — "1.4 MB"
 *  is a size and "1434 KB" is a number. */
function size(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}
