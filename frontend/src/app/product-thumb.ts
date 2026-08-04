import { NgTemplateOutlet } from '@angular/common';
import { Component, computed, inject, input, signal } from '@angular/core';
import { MatIconModule } from '@angular/material/icon';
import { MatMenuModule } from '@angular/material/menu';
import { MatSnackBar } from '@angular/material/snack-bar';

import { ImagePickerDirective } from './image-picker';
import { LifeApi } from './life-api';
import { ProductImages, showThumb } from './product-image';

/**
 * The native clipboard port the Android WebView wrapper injects (see the app's
 * MainActivity). Present only inside the custom app; absent in a browser.
 *
 * A message port rather than the one plain method it used to be. The wrapper
 * injects it with `WebViewCompat.addWebMessageListener`, whose origin rules keep
 * it out of any frame that isn't this app — the API it replaced was injected into
 * every frame in the WebView, including iframes, and what sat behind it was the
 * system clipboard.
 */
interface AndroidClipboard {
  postMessage(message: string): void;
  addEventListener(type: 'message', listener: (event: { data: string }) => void): void;
  removeEventListener(type: 'message', listener: (event: { data: string }) => void): void;
}
// Declared rather than asserted at the read. An ambient declaration is what a
// foreign API contract is for: the shape is stated once, so the read below is
// an ordinary typed property access — and the runtime check that follows is
// about PRESENCE (are we in the wrapper?), which is the only thing in doubt.
declare global {
  var AndroidClipboard: AndroidClipboard | undefined;
}
/** The port, if the installed app speaks this page's version of it. An older app
 *  injects the previous shape (a plain `readImage()`), which has no
 *  `postMessage` — that reads as no bridge, so "Paste copied image" is simply
 *  absent until the app is updated rather than throwing when tapped. */
function androidClipboard(): AndroidClipboard | undefined {
  const bridge = globalThis.AndroidClipboard;
  return typeof bridge?.postMessage === 'function' ? bridge : undefined;
}

/** How long to wait for the phone to answer before giving up. The read is a
 *  clipboard fetch on the UI thread, so it is immediate in practice; this exists
 *  so a tap can never hang the paste action forever. */
const CLIPBOARD_TIMEOUT_MS = 2000;

/** The image on the system clipboard as a `data:` URL, or null when there is
 *  none (the native side answers with an empty string) or no app to ask. */
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

/** Client ceiling, mirrors the backend's 5 MiB cap. */
const MAX_BYTES = 5 * 1024 * 1024;

/** A product thumbnail that doubles as a one-tap image picker (see
 *  [[ImagePickerDirective]]). Drop it into any list row:
 *
 *      <app-product-thumb matListItemAvatar [barcode]="it.barcode"
 *                         [hasImage]="it.has_image" />
 *
 *  It renders the cached image, or an `add_a_photo` placeholder when the barcoded
 *  product has none, and owns the whole replace flow (pick → upload → reload) so
 *  the host list doesn't have to. Inside the Android app it also offers "Paste
 *  copied image" (from the system clipboard, e.g. an image copied in Chrome).
 *  A barcodeless item linked to a shop product (`[productId]`) shows that
 *  product's image read-only — there's no barcode to attach a replacement to.
 *  An item with neither a barcode nor a linked product gets a plain, inert icon. */
@Component({
  selector: 'app-product-thumb',
  templateUrl: './product-thumb.html',
  styleUrl: './product-thumb.scss',
  imports: [MatIconModule, MatMenuModule, NgTemplateOutlet, ImagePickerDirective],
})
export class ProductThumb {
  readonly barcode = input<string | null>(null);
  /** A linked catalog product id. Used to show a barcodeless shop product's
   *  image (addressed by id); such a product has no barcode to replace, so the
   *  thumbnail is inert (view-only). */
  readonly productId = input<number | null>(null);
  /** Catalog hint: does a cached image exist? `undefined` = unknown, try anyway. */
  readonly hasImage = input<boolean | undefined>(undefined);

  private images = inject(ProductImages);
  private api = inject(LifeApi);
  private snack = inject(MatSnackBar);

  /** True in the custom Android app (the clipboard bridge is present) — there we
   *  offer a menu (Paste / Choose); in a browser, tapping picks a file directly. */
  protected readonly inApp = androidClipboard() !== undefined;

  private failed = signal(false);
  /** Set once a replace succeeds, so the image shows even if `hasImage` was false. */
  private uploaded = signal(false);
  protected readonly busy = signal(false);

  /** The image URL to show, or null to fall back to the placeholder icon. */
  protected readonly src = computed<string | null>(() => {
    const barcode = this.barcode();
    if (barcode) {
      // Barcoded product: the (replaceable) image lives at /products/{barcode}/image.
      if (this.uploaded()) return this.images.url(barcode);
      if (!showThumb({ barcode, has_image: this.hasImage() }, this.failed())) return null;
      return this.images.url(barcode);
    }
    // No barcode, but a linked shop product may carry an image addressed by id.
    const productId = this.productId();
    if (
      productId &&
      showThumb({ barcode: null, product_id: productId, has_image: this.hasImage() }, this.failed())
    ) {
      return this.api.productImageByIdUrl(productId);
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

  /** Upload the image currently on the system clipboard (Android app only). */
  protected async pasteFromClipboard(): Promise<void> {
    const dataUrl = await readClipboardImage();
    if (!dataUrl) {
      this.snack.open('No image on the clipboard — use “Copy image” first.', 'OK', {
        duration: 4000,
      });
      return;
    }
    const blob = await (await fetch(dataUrl)).blob();
    if (blob.size > MAX_BYTES) {
      this.snack.open('That image is larger than 5 MB.', 'OK', { duration: 4000 });
      return;
    }
    this.onPicked(blob);
  }
}
