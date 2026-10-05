import { NgTemplateOutlet } from '@angular/common';
import { Component, computed, inject, input, signal } from '@angular/core';
import { MatIconModule } from '@angular/material/icon';
import { MatMenuModule } from '@angular/material/menu';
import { MatSnackBar } from '@angular/material/snack-bar';

import { ImagePickerDirective, MAX_IMAGE_BYTES } from './image-picker';
import { ProductImages, showThumb } from './product-image';

/** The Android wrapper's clipboard port; absent in a browser. */
interface AndroidClipboard {
  postMessage(message: string): void;
  addEventListener(type: 'message', listener: (event: { data: string }) => void): void;
  removeEventListener(type: 'message', listener: (event: { data: string }) => void): void;
}
declare global {
  var AndroidClipboard: AndroidClipboard | undefined;
}
/** An older app's port lacks `postMessage` and reads as absent. */
function androidClipboard(): AndroidClipboard | undefined {
  const bridge = globalThis.AndroidClipboard;
  return typeof bridge?.postMessage === 'function' ? bridge : undefined;
}

/** So a paste can never hang. */
const CLIPBOARD_TIMEOUT_MS = 2000;

/** The clipboard image as a `data:` URL, or null. */
function readClipboardImage(): Promise<string | null> {
  const bridge = androidClipboard();
  if (!bridge) return Promise.resolve(null);
  return new Promise((resolve) => {
    const done = (value: string | null) => {
      clearTimeout(timer);
      bridge.removeEventListener('message', onMessage);
      resolve(value);
    };
    const onMessage = (event: { data: string }) => done(event.data || null);
    const timer = setTimeout(() => done(null), CLIPBOARD_TIMEOUT_MS);
    bridge.addEventListener('message', onMessage);
    bridge.postMessage(JSON.stringify({ op: 'readImage' }));
  });
}

/** A product thumbnail that is also a tap-to-replace picker. A barcodeless
 *  product's image is shown read-only. */
@Component({
  selector: 'app-product-thumb',
  templateUrl: './product-thumb.html',
  styleUrl: './product-thumb.scss',
  imports: [MatIconModule, MatMenuModule, NgTemplateOutlet, ImagePickerDirective],
})
export class ProductThumb {
  readonly barcode = input<string | null>(null);
  readonly productId = input<number | null>(null);
  /** `undefined` means unknown: try anyway. */
  readonly hasImage = input<boolean | undefined>(undefined);

  private images = inject(ProductImages);
  private snack = inject(MatSnackBar);

  /** In the app a tap offers Paste or Choose; in a browser it picks a file. */
  protected readonly inApp = androidClipboard() !== undefined;

  private failed = signal(false);
  /** Shows the image after a replace even if `hasImage` was false. */
  private uploaded = signal(false);
  protected readonly busy = signal(false);

  protected readonly src = computed<string | null>(() => {
    const barcode = this.barcode();
    if (barcode) {
      if (this.uploaded()) return this.images.url(barcode);
      if (!showThumb({ barcode, has_image: this.hasImage() }, this.failed())) return null;
      return this.images.url(barcode);
    }
    const productId = this.productId();
    if (
      productId &&
      showThumb({ barcode: null, product_id: productId, has_image: this.hasImage() }, this.failed())
    ) {
      return this.images.urlById(productId);
    }
    return null;
  });

  protected onError(): void {
    this.failed.set(true);
  }

  protected onPicked(blob: Blob): void {
    const barcode = this.barcode();
    if (!barcode) return;
    this.busy.set(true);
    this.images.replace(barcode, blob).subscribe({
      next: () => {
        this.failed.set(false);
        this.uploaded.set(true);
        this.busy.set(false);
      },
      error: () => {
        this.busy.set(false);
        this.snack.open('Could not save the image.', 'OK', { duration: 4000 });
      },
    });
  }

  protected onPickError(message: string): void {
    this.snack.open(message, 'OK', { duration: 4000 });
  }

  protected async pasteFromClipboard(): Promise<void> {
    const dataUrl = await readClipboardImage();
    if (!dataUrl) {
      this.snack.open('No image on the clipboard — use “Copy image” first.', 'OK', {
        duration: 4000,
      });
      return;
    }
    const blob = await (await fetch(dataUrl)).blob();
    if (blob.size > MAX_IMAGE_BYTES) {
      this.snack.open('That image is larger than 5 MB.', 'OK', { duration: 4000 });
      return;
    }
    this.onPicked(blob);
  }
}
