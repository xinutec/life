import { TestBed } from '@angular/core/testing';
import { MAT_DIALOG_DATA, MatDialogRef } from '@angular/material/dialog';
import { Observable, of, throwError } from 'rxjs';
import { describe, expect, it, vi } from 'vitest';

import { Feedback } from '../../shared/feedback';
import { LifeApi } from '../../life-api';
import { Item, ItemFile } from '../../models';
import { FilesDialog, FilesDialogData } from './files-dialog';

const ITEM: Item = {
  id: 93,
  product_id: null,
  name: 'Dishwasher',
  brand: null,
  category: 'appliance',
  quantity: null,
  unit: null,
  expiry: null,
  expiry_precision: 'day',
  location_id: 2,
  barcode: null,
  has_image: false,
};

function file(over: Partial<ItemFile> = {}): ItemFile {
  return {
    id: 1,
    item_id: 93,
    purchase_id: null,
    name: 'manual.pdf',
    mime: 'application/pdf',
    size_bytes: 2 * 1024 * 1024,
    created_at: new Date().toISOString(),
    ...over,
  };
}

function setup(
  opts: { files?: ItemFile[]; addItemFile?: () => Observable<unknown> } = {},
) {
  const itemFiles = vi.fn(() => of(opts.files ?? []));
  const addItemFile = vi.fn(opts.addItemFile ?? (() => of(file())));
  const deleteItemFile = vi.fn(() => of(undefined));
  const feedback = { notify: vi.fn(), error: vi.fn() };
  TestBed.configureTestingModule({
    imports: [FilesDialog],
    providers: [
      { provide: MatDialogRef, useValue: { close: vi.fn() } },
      { provide: MAT_DIALOG_DATA, useValue: { item: ITEM } satisfies FilesDialogData },
      {
        provide: LifeApi,
        useValue: {
          itemFiles,
          addItemFile,
          deleteItemFile,
          fileUrl: (i: number, f: number) => `/api/items/${i}/files/${f}`,
        },
      },
      { provide: Feedback, useValue: feedback },
    ],
  });
  const fixture = TestBed.createComponent(FilesDialog);
  fixture.detectChanges();
  return { cmp: fixture.componentInstance, itemFiles, addItemFile, deleteItemFile, feedback };
}

/** A file picker's change event, with the input the component must clear. */
function pickEvent(f: File): { event: Event; input: HTMLInputElement } {
  const input = document.createElement('input');
  input.type = 'file';
  const list = { 0: f, length: 1, item: (i: number) => (i === 0 ? f : null) };
  Object.defineProperty(input, 'files', { value: list, writable: true });
  return { event: { target: input } as unknown as Event, input };
}

describe('FilesDialog', () => {
  it('refuses a file bigger than the server would take, before uploading it', () => {
    // Checked here as well as on the server: the round trip is the expensive
    // part on a phone, and learning "too big" after sending 40 MB over mobile
    // data is the worst possible order to learn it in.
    const { cmp, addItemFile, feedback } = setup();
    const big = new File(['x'], 'huge.pdf');
    Object.defineProperty(big, 'size', { value: 11 * 1024 * 1024 });
    cmp.pick(pickEvent(big).event);
    expect(addItemFile).not.toHaveBeenCalled();
    expect(feedback.error).toHaveBeenCalledWith(expect.stringContaining('10 MB'));
  });

  it('refuses an empty file rather than storing nothing', () => {
    const { cmp, addItemFile, feedback } = setup();
    const empty = new File([], 'nothing.pdf');
    Object.defineProperty(empty, 'size', { value: 0 });
    cmp.pick(pickEvent(empty).event);
    expect(addItemFile).not.toHaveBeenCalled();
    expect(feedback.error).toHaveBeenCalled();
  });

  it('clears the input, so the same file can be picked again after a failure', () => {
    // A file input does not fire `change` when the same file is chosen twice.
    // Without clearing it, a failed upload cannot be retried by picking the same
    // file — the button simply stops responding, which reads as broken.
    const { cmp } = setup({ addItemFile: () => throwError(() => new Error('nope')) });
    const f = new File(['x'], 'receipt.pdf');
    Object.defineProperty(f, 'size', { value: 1024 });
    const { event, input } = pickEvent(f);
    input.value = '';
    cmp.pick(event);
    expect(input.value).toBe('');
    expect(cmp.uploading()).toBe(false);
  });

  it('re-reads after attaching, rather than guessing what the server stored', () => {
    const { cmp, itemFiles } = setup();
    const before = itemFiles.mock.calls.length;
    const f = new File(['x'], 'receipt.pdf');
    Object.defineProperty(f, 'size', { value: 1024 });
    cmp.pick(pickEvent(f).event);
    expect(itemFiles.mock.calls.length).toBe(before + 1);
  });

  it('shows a size and a link, and tells a PDF from a picture', () => {
    const { cmp } = setup({
      files: [
        file({ id: 4, name: 'receipt.png', mime: 'image/png', size_bytes: 300 * 1024 }),
        file({ id: 5, name: 'manual.pdf', mime: 'application/pdf' }),
      ],
    });
    const [png, pdf] = cmp.rows();
    expect(png.icon).toBe('image');
    expect(pdf.icon).toBe('picture_as_pdf');
    expect(png.href).toBe('/api/items/93/files/4');
    expect(png.full).toBe('receipt.png');
    // "1434 KB" is a number; "1.4 MB" is a size.
    expect(pdf.detail).toContain('2.0 MB');
    expect(png.detail).toContain('300 KB');
  });

it('splits a phone-named scan so the extension cannot be trimmed away', () => {
    // The head is what CSS is allowed to eat; the tail is pinned. A character
    // budget was tried first and was wrong by 60% — the row is 192px and 34
    // characters render at 307 — which is why the split is structural.
    const { cmp } = setup({
      files: [file({ name: 'IMG_20240315_143022_receipt_dishwasher_manual.pdf' })],
    });
    const [row] = cmp.rows();
    expect(row.tail).toBe('r_manual.pdf');
    expect(row.tail.endsWith('.pdf')).toBe(true);
    expect(row.head + row.tail).toBe(row.full);
    expect(row.full).toBe('IMG_20240315_143022_receipt_dishwasher_manual.pdf');
  });

  it('leaves a short name whole, with nothing in the head to trim', () => {
    const { cmp } = setup({ files: [file({ name: 'receipt.pdf' })] });
    const [row] = cmp.rows();
    expect(row.head).toBe('');
    expect(row.tail).toBe('receipt.pdf');
  });

  it('keeps the row and says so when a removal fails', () => {
    const { cmp, feedback } = setup({ files: [file({ id: 4 })] });
    TestBed.inject(LifeApi).deleteItemFile = vi.fn(() => throwError(() => new Error('nope')));
    cmp.remove(4);
    expect(feedback.error).toHaveBeenCalled();
    expect(cmp.rows()).toHaveLength(1);
  });
});
