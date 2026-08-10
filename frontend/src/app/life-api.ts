import { HttpClient } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable } from 'rxjs';
import {
  AsdaHit,
  ConflictEntry,
  ConflictKind,
  CoverageQuery,
  FieldChoice,
  HouseScene,
  Item,
  Loc,
  Me,
  PriceInput,
  Product,
  ProductDetail,
  Recipe,
  RecipeIngredient,
  Remembered,
  RowCoverage,
  SeenListing,
  ShopFind,
  ShoppingItem,
  Source,
  SuggestEmotionsRequest,
  SuggestEmotionsResponse,
  CookedLine,
  TelemetryEvent,
  TrashEntry,
  TrashKind,
  WarmEmotionsRequest,
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

  /** Ask the backend to rank feelings against a check-in note (Claude, online-only).
   *  Best-effort: with no API key or offline it resolves to an empty list, so the
   *  picker degrades to the plain wheel. */
  suggestEmotions(body: SuggestEmotionsRequest): Observable<SuggestEmotionsResponse> {
    return this.http.post<SuggestEmotionsResponse>('/api/wellbeing/suggest-emotions', body);
  }

  /** Preload the model for a suggestion about to be asked for (fired when a note
   *  starts being written). Fire-and-forget: the answer is still produced by the
   *  real suggestEmotions call; this only makes it warm. */
  warmEmotions(body: WarmEmotionsRequest): Observable<void> {
    return this.http.post<void>('/api/wellbeing/warm-emotions', body);
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
  /** Take an amount out of a stock row ("I used 200g of flour"). `unit` must be
   *  the row's own — the server refuses a mismatch rather than converting, so
   *  send what the item says, not what the recipe said. */
  useItem(id: number, quantity: number, unit: string | null): Observable<Item> {
    return this.http.post<Item>(`/api/items/${id}/use`, { quantity, unit });
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
  findAtShop(id: number, source: Source): Observable<ShopFind> {
    return this.http.get<ShopFind>(
      `/api/products/id/${id}/find/${encodeURIComponent(source)}`,
    );
  }
  /** Teach the backend what this device's WebView saw at a shop the server
   *  can't reach itself (Waitrose is behind a bot-wall). Every listing a hunt
   *  passed over is worth reporting, not just the one that matched: each is a
   *  durable barcode → shop-id fact that spares the next hunt a page load. */
  rememberShopListings(source: Source, listings: SeenListing[]): Observable<Remembered> {
    return this.http.post<Remembered>(
      `/api/products/shop/${encodeURIComponent(source)}/listings`,
      listings,
    );
  }
  /** Where each Buy-list row is known to be SOLD — the shops holding a listing
   *  for its product, plus the shops a past query showed carrying its barcode.
   *  Memory only: no shop is contacted, so this can run on every list load.
   *  Never a stock check, and an empty `sources` means we know nothing about
   *  that row rather than that nowhere sells it. */
  shopCoverage(rows: CoverageQuery[]): Observable<RowCoverage[]> {
    return this.http.post<RowCoverage[]>('/api/shopping/coverage', rows);
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
    source: Source;
    external_id: string;
    name: string;
    brand?: string | null;
    /** The pack the shop sells, as the shop writes it ("400G"). Comes back
     *  parsed as `Product.pack`, which is what fills a new stock row's amount. */
    quantity_label?: string | null;
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
  syncListing(id: number, source: Source, externalId: string): Observable<Product> {
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
   *  decision adopts a source's value ({field, choice: source}), keeps the
   *  current one ({field, choice: 'keep'}), or sets our own typed value
   *  ({field, choice: 'user', value}).
   *
   *  `FieldChoice` is the backend's own request type, so a field name or choice
   *  it doesn't accept can't be sent — the 400 it used to answer with is now a
   *  compile error. Returns the re-read detail with the divergence list updated. */
  reconcile(id: number, decisions: FieldChoice[]): Observable<ProductDetail> {
    return this.http.post<ProductDetail>(`/api/products/id/${id}/reconcile`, decisions);
  }
  /** Store facts a shop's product PAGE carries but its API doesn't — Asda's
   *  Brandbank nutrition/ingredients/allergens/dietary, fetched by the hidden
   *  WebView (the page is bot-walled) and parsed server-side. `ean` is the page's
   *  own barcode; the backend rejects a blob whose barcode isn't this product's.
   *  Returns the re-read detail. */
  submitFacts(
    id: number,
    body: { source: Source; ean: string; blob: string },
  ): Observable<ProductDetail> {
    return this.http.post<ProductDetail>(`/api/products/id/${id}/facts`, body);
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
  /** Cook it: take every ingredient out of the cupboard. Answers with one line
   *  per ingredient — INCLUDING the ones nothing happened to, which is the whole
   *  contract (see recipes::cooking). */
  cookRecipe(id: number): Observable<CookedLine[]> {
    return this.http.post<CookedLine[]>(`/api/recipes/${id}/cook`, {});
  }
  cookable(): Observable<Recipe[]> {
    return this.http.get<Recipe[]>('/api/cookable');
  }
  shoppingList(id: number): Observable<RecipeIngredient[]> {
    return this.http.get<RecipeIngredient[]>(`/api/recipes/${id}/shopping-list`);
  }
}
