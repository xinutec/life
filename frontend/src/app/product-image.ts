import { Injectable, inject, signal } from '@angular/core';
import { Observable } from 'rxjs';
import { tap } from 'rxjs/operators';
import { LifeApi } from './life-api';

/** What decides whether a row shows a product thumbnail. */
export interface ThumbSource {
  barcode: string | null;
  product_id?: number | null;
  /** Unknown (`undefined`) means try anyway; a load error falls back. */
  has_image?: boolean;
}

export function showThumb(it: ThumbSource, failed: boolean): boolean {
  const linked = !!it.barcode || !!it.product_id;
  return !failed && linked && it.has_image !== false;
}

/** A `?v=` cache-buster per image, app-wide: a page keeps the bytes it loaded
 *  for a URL whatever the headers say. */
@Injectable({ providedIn: 'root' })
export class ProductImages {
  private api = inject(LifeApi);
  /** Keyed by both URLs an image has: `b:<barcode>` and `p:<id>`. */
  private version = signal<ReadonlyMap<string, number>>(new Map());

  url(barcode: string): string {
    return this.api.productImageUrl(barcode, this.version().get(`b:${barcode}`));
  }

  urlById(id: number): string {
    return this.api.productImageByIdUrl(id, this.version().get(`p:${id}`));
  }

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

  replace(barcode: string, blob: Blob, id?: number): Observable<void> {
    return this.api
      .uploadProductImage(barcode, blob)
      .pipe(tap(() => this.changed({ barcode, id })));
  }
}
