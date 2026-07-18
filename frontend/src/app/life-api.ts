import { HttpClient } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable } from 'rxjs';
import {
  AsdaHit,
  ConflictEntry,
  ConflictKind,
  HouseScene,
  Item,
  Loc,
  Me,
  PriceInput,
  Product,
  ProductDetail,
  Recipe,
  RecipeIngredient,
  ShopFind,
  ShoppingItem,
  TelemetryEvent,
  TrashEntry,
  TrashKind,
} from './models';

/** Thin client over the life backend. Same-origin in prod; via the dev proxy
 *  (proxy.conf.json) in `ng serve`. Session cookie rides along automatically. */
@Injectable({ providedIn: 'root' })
export class LifeApi {
  private http = inject(HttpClient);

  me(): Observable<Me> {
    return this.http.get<Me>('/api/me');
  }
  logout(): Observable<unknown> {
    return this.http.post('/logout', {});
  }

  locations(): Observable<Loc[]> {
    return this.http.get<Loc[]>('/api/locations');
  }
  createLocation(body: Partial<Loc>): Observable<Loc> {
    return this.http.post<Loc>('/api/locations', body);
  }

  items(): Observable<Item[]> {
    return this.http.get<Item[]>('/api/items');
  }
  createItem(body: Partial<Item>): Observable<Item> {
    return this.http.post<Item>('/api/items', body);
  }
  updateItem(id: number, body: Partial<Item>): Observable<Item> {
    return this.http.patch<Item>(`/api/items/${id}`, body);
  }
  deleteItem(id: number): Observable<unknown> {
    return this.http.delete(`/api/items/${id}`);
  }
  moveItem(id: number, locationId: number | null): Observable<Item> {
    return this.http.post<Item>(`/api/items/${id}/move`, { location_id: locationId });
  }
  deleteLocation(id: number): Observable<unknown> {
    return this.http.delete(`/api/locations/${id}`);
  }

  house(): Observable<HouseScene> {
    return this.http.get<HouseScene>('/api/house');
  }

  shopping(): Observable<ShoppingItem[]> {
    return this.http.get<ShoppingItem[]>('/api/shopping');
  }
  addShopping(body: Partial<ShoppingItem>): Observable<ShoppingItem> {
    return this.http.post<ShoppingItem>('/api/shopping', body);
  }
  updateShopping(id: number, body: Partial<ShoppingItem>): Observable<ShoppingItem> {
    return this.http.patch<ShoppingItem>(`/api/shopping/${id}`, body);
  }
  deleteShopping(id: number): Observable<unknown> {
    return this.http.delete(`/api/shopping/${id}`);
  }
  buyShopping(id: number): Observable<Item> {
    return this.http.post<Item>(`/api/shopping/${id}/buy`, {});
  }

  /** Look up (and cache) a product by barcode via Open Food Facts. */
  lookupProduct(barcode: string): Observable<Product> {
    return this.http.get<Product>(`/api/products/${encodeURIComponent(barcode)}`);
  }
  /** Catalog name/brand substring search (the product picker's catalog tier). */
  searchProducts(q: string): Observable<Product[]> {
    return this.http.get<Product[]>('/api/products', { params: { q } });
  }
  /** Live name search against Asda's storefront (the picker's Asda tier). Unlike
   *  the Waitrose shop tier, this is a plain backend call — no app bridge — so
   *  it works in the browser too. */
  searchAsda(q: string): Observable<AsdaHit[]> {
    return this.http.get<AsdaHit[]>('/api/products/shop/asda', { params: { q } });
  }
  /** Does this shop carry this product's barcode? The backend answers from its
   *  own memory of past shop queries when it can, so a repeat lookup costs the
   *  shop nothing; only a miss goes out to search. Matching is by barcode
   *  server-side — a shop's relevance ranking is not evidence of identity. */
  findAtShop(id: number, source: string): Observable<ShopFind> {
    return this.http.get<ShopFind>(
      `/api/products/id/${id}/find/${encodeURIComponent(source)}`,
    );
  }
  /** Fold a batch of client activity events (navigations, taps) into the backend
   *  log stream. Best-effort telemetry — see `Telemetry`; callers ignore the
   *  result. */
  sendTelemetry(events: TelemetryEvent[]): Observable<void> {
    return this.http.post<void>('/api/telemetry', events);
  }
  /** URL of the cached product image (use directly as <img src>). Pass a
   *  `version` after a replace to bust the browser/service-worker cache. */
  productImageUrl(barcode: string, version?: number): string {
    const base = `/api/products/${encodeURIComponent(barcode)}/image`;
    return version ? `${base}?v=${version}` : base;
  }
  /** Replace the cached image for a barcode with raw image bytes. The blob's
   *  own mime rides along as Content-Type; the backend re-validates it. */
  uploadProductImage(barcode: string, blob: Blob): Observable<void> {
    return this.http.put<void>(`/api/products/${encodeURIComponent(barcode)}/image`, blob, {
      headers: { 'Content-Type': blob.type },
    });
  }
  /** Import a product from an external source (a shop) into the catalog, keyed on
   *  (source, external_id). The backend fetches + stores the image server-side. */
  importProduct(body: {
    source: string;
    external_id: string;
    name: string;
    brand?: string | null;
    /** The EAN when the source knows it — merges shop + Open Food Facts data
     *  onto one canonical product by barcode. */
    barcode?: string | null;
    image_url?: string | null;
    /** Price the source quoted — appended to the product's price history. */
    price?: PriceInput | null;
  }): Observable<Product> {
    return this.http.post<Product>('/api/products/import', body);
  }
  /** Pull a product's listing at a shop and store what it says — price, the
   *  shop's lifestyle tags, pack size, clean name. Same call attaches a shop for
   *  the first time and refreshes it later; the backend fetches shop-side and
   *  enforces that the listing's barcode really is this product's. */
  syncListing(id: number, source: string, externalId: string): Observable<Product> {
    return this.http.post<Product>(`/api/products/id/${id}/listings`, {
      source,
      external_id: externalId,
    });
  }
  /** Everything the product page shows, in one fetch: the canonical product,
   *  its per-source listings (deep links resolved), latest price per shop
   *  (cheapest first), and its nutrition/ingredients/allergen/dietary facts. */
  getProductDetail(id: number): Observable<ProductDetail> {
    return this.http.get<ProductDetail>(`/api/products/id/${id}`);
  }
  /** Settle where the product's sources disagree with its canonical row: each
   *  decision adopts a source's value ({field, choice: source}) or keeps the
   *  current one ({field, choice: 'keep'}). Returns the re-read detail with the
   *  divergence list updated. */
  reconcile(
    id: number,
    decisions: { field: string; choice: string }[],
  ): Observable<ProductDetail> {
    return this.http.post<ProductDetail>(`/api/products/id/${id}/reconcile`, decisions);
  }
  /** URL of a catalog image addressed by product id — for barcodeless shop
   *  products, which have no /products/{barcode}/image URL. */
  productImageByIdUrl(id: number, version?: number): string {
    const base = `/api/products/id/${id}/image`;
    return version ? `${base}?v=${version}` : base;
  }

  /** Unresolved same-field sync conflicts, newest first. */
  conflicts(): Observable<ConflictEntry[]> {
    return this.http.get<ConflictEntry[]>('/api/conflicts');
  }
  /** Record a client-detected same-field conflict (values JSON-encoded). */
  reportConflict(body: {
    kind: ConflictKind;
    ulid: string;
    field: string;
    label: string;
    mine: string;
    theirs: string;
  }): Observable<void> {
    return this.http.post<void>('/api/conflicts', body);
  }
  /** Mark a conflict handled — keep-mine and use-other both end here. */
  resolveConflict(id: number): Observable<void> {
    return this.http.post<void>(`/api/conflicts/${id}/resolve`, {});
  }

  /** Everything deleted (all kinds), newest first. Nothing is ever purged. */
  trash(): Observable<TrashEntry[]> {
    return this.http.get<TrashEntry[]>('/api/trash');
  }
  /** Restore one trash entry — the deliberate undelete path (also used by the
   *  Undo snackbars). `ref` is the id (item/location/recipe) or ulid
   *  (shopping/todo) from the entry. */
  restoreTrash(kind: TrashKind, ref: string): Observable<void> {
    return this.http.post<void>(
      `/api/trash/${kind}/${encodeURIComponent(ref)}/restore`,
      {},
    );
  }

  recipes(): Observable<Recipe[]> {
    return this.http.get<Recipe[]>('/api/recipes');
  }
  createRecipe(body: Partial<Recipe>): Observable<Recipe> {
    return this.http.post<Recipe>('/api/recipes', body);
  }
  updateRecipe(id: number, body: Partial<Recipe>): Observable<Recipe> {
    return this.http.put<Recipe>(`/api/recipes/${id}`, body);
  }
  deleteRecipe(id: number): Observable<unknown> {
    return this.http.delete(`/api/recipes/${id}`);
  }
  cookable(): Observable<Recipe[]> {
    return this.http.get<Recipe[]>('/api/cookable');
  }
  shoppingList(id: number): Observable<RecipeIngredient[]> {
    return this.http.get<RecipeIngredient[]>(`/api/recipes/${id}/shopping-list`);
  }
}
