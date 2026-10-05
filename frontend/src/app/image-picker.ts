import { Directive, input, output, signal } from '@angular/core';

/** Client-side ceiling, mirrors the backend's 5 MiB cap so we reject early.
 *  Every picture path checks this one, so none can upload what another refuses. */
export const MAX_IMAGE_BYTES = 5 * 1024 * 1024;

function firstImage(files: Iterable<File> | undefined): File | undefined {
  return Array.from(files ?? []).find((f) => f.type.startsWith('image/'));
}

/** Makes its host an image picker: tap, Enter or Space opens the file dialog,
 *  and paste and drop work too. */
@Directive({
  selector: '[appImagePicker]',
  exportAs: 'imagePicker',
  host: {
    role: 'button',
    tabindex: '0',
    'aria-label': 'Replace image',
    '[class.drag-over]': 'dragOver()',
    '(click)': 'onActivate()',
    '(keydown.enter)': 'onKey($event)',
    '(keydown.space)': 'onKey($event)',
    '(paste)': 'onPaste($event)',
    '(dragover)': 'onDragOver($event)',
    '(dragleave)': 'dragOver.set(false)',
    '(drop)': 'onDrop($event)',
  },
})
export class ImagePickerDirective {
  readonly imagePicked = output<Blob>();
  readonly pickError = output<string>();
  readonly dragOver = signal(false);
  /** False when the host opens the dialog itself, e.g. from a menu. */
  readonly clickToOpen = input(true);

  onActivate(): void {
    if (this.clickToOpen()) this.openDialog();
  }

  openDialog(): void {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'image/*';
    input.style.display = 'none';
    input.addEventListener('change', () => {
      this.accept(input.files?.[0]);
      input.remove();
    });
    document.body.appendChild(input);
    input.click();
  }

  onKey(e: Event): void {
    e.preventDefault();
    this.onActivate();
  }

  onPaste(e: ClipboardEvent): void {
    const file = Array.from(e.clipboardData?.items ?? [])
      .find((i) => i.type.startsWith('image/'))
      ?.getAsFile();
    if (file) {
      e.preventDefault();
      this.accept(file);
    }
  }

  onDragOver(e: DragEvent): void {
    e.preventDefault();
    this.dragOver.set(true);
  }

  onDrop(e: DragEvent): void {
    e.preventDefault();
    this.dragOver.set(false);
    this.accept(firstImage(e.dataTransfer?.files));
  }

  private accept(file: File | undefined | null): void {
    if (!file) return;
    if (!file.type.startsWith('image/')) {
      this.pickError.emit('That’s not an image.');
      return;
    }
    if (file.size > MAX_IMAGE_BYTES) {
      this.pickError.emit('Image is larger than 5 MB.');
      return;
    }
    this.imagePicked.emit(file);
  }
}
