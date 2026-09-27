import { Injectable, inject, signal } from '@angular/core';
import { Observable } from 'rxjs';
import { tap } from 'rxjs/operators';
import { LifeApi } from './life-api';

/** Single source for "should this row show a product thumbnail?". Shopping,
 *  Inventory and All-items each render the <img>/icon themselves (Material list
 *  slots differ), but the *rule* lives here. */
export interface ThumbSource {
  barcode: string | null;
  /** A linked catalog product with no EAN (a shop product): its image is
   *  addressed by product id, not barcode. */
  product_id?: number | null;
  /** From the catalog: a cached image exists. `undefined` (e.g. shopping rows,
   *  which don't carry it) = unknown → try anyway, the load-error fallback
   *  handles a miss. */
  has_image?: boolean;
}

export function showThumb(it: ThumbSource, failed: boolean): boolean {
  const linked = !!it.barcode || !!it.product_id;
  return !failed && linked && it.has_image !== false;
}

/** Shared cache-buster for product images. An image lives at a stable URL, and
 *  a page keeps showing the bytes it already loaded for a URL whatever the
 *  headers say — so after a change, bumping a version and appending it as `?v=`
 *  forces a reload. App-wide, so a change in one view refreshes every view. */
@Injectable({ providedIn: 'root' })
export class ProductImages {
  private api = inject(LifeApi);
  /** Keyed `b:<barcode>` and `p:<product id>`: the two URLs an image has. */
  private version = signal<ReadonlyMap<string, number>>(new Map());

  /** `<img src>` for a barcode's image, cache-busted after any change. */
  url(barcode: string): string {
    return this.api.productImageUrl(barcode, this.version().get(`b:${barcode}`));
  }

  /** `<img src>` for a product's image by id, cache-busted after any change. */
  urlById(id: number): string {
    return this.api.productImageByIdUrl(id, this.version().get(`p:${id}`));
  }

  /** The picture changed server-side: reload it under both its URLs. */
  changed(product: { id?: number; barcode?: string | null }): void {
    const keys = [
      product.barcode ? `b:${product.barcode}` : null,
      product.id != null ? `p:${product.id}` : null,
    ].filter((k) => k !== null);
    this.version.update((m) => {
      const next = new Map(m);
      for (const k of keys) next.set(k, (m.get(k) ?? 0) + 1);
      return next;
    });
  }

  /** Upload new bytes; on success bump the buster so every `<img>` reloads. */
  replace(barcode: string, blob: Blob, id?: number): Observable<void> {
    return this.api
      .uploadProductImage(barcode, blob)
      .pipe(tap(() => this.changed({ barcode, id })));
  }
}
