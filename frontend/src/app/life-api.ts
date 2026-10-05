import { HttpClient } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { Observable } from 'rxjs';
import {
  AsdaHit,
  BinDay,
  ConflictEntry,
  ConnectStarted,
  ConnectState,
  ConflictKind,
  CoverageQuery,
  FieldChoice,
  HouseScene,
  Item,
  ItemHistory,
  Loc,
  Me,
  NewPurchase,
  PlannedTrip,
  PriceInput,
  Product,
  ItemFile,
  ProductDetail,
  Purchase,
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
  TrashEntry,
  TrashKind,
  WarmEmotionsRequest,
} from './models';

/** The life backend's REST API. */
@Injectable({ providedIn: 'root' })
export class LifeApi {
  private http = inject(HttpClient);

  me(): Observable<Me> {
    return this.http.get<Me>('/api/me');
  }
  logout(): Observable<unknown> {
    return this.http.post('/logout', {});
  }

  /** Emotions suggested for a note by the local model; none without one. */
  suggestEmotions(body: SuggestEmotionsRequest): Observable<SuggestEmotionsResponse> {
    return this.http.post<SuggestEmotionsResponse>('/api/wellbeing/suggest-emotions', body);
  }

  /** Preload the model while a note is still being written. */
  warmEmotions(body: WarmEmotionsRequest): Observable<void> {
    return this.http.post<void>('/api/wellbeing/warm-emotions', body);
  }

  /** Start Nextcloud Login Flow v2; returns the approval URL before the grant
   *  completes. {@link nextcloudStatus} says when it has. */
  nextcloudConnect(): Observable<ConnectStarted> {
    return this.http.post<ConnectStarted>('/api/nextcloud/connect/init', {});
  }
  /** Whether the app password is in hand. */
  nextcloudStatus(): Observable<ConnectState> {
    return this.http.get<ConnectState>('/api/nextcloud/connect/status');
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
  /** "I used 200 g". `unit` must be the row's own; the server never converts. */
  useItem(id: number, quantity: number, unit: string | null): Observable<Item> {
    return this.http.post<Item>(`/api/items/${id}/use`, { quantity, unit });
  }

  /** Record that an item is running out. */
  markLow(id: number): Observable<void> {
    return this.http.post<void>(`/api/items/${id}/low`, {});
  }

  /** The same, by identity; nothing happens if no item matches. */
  markLowByIdentity(identity: {
    name: string;
    barcode: string | null;
    product_id: number | null;
  }): Observable<void> {
    return this.http.post<void>('/api/items/low', identity);
  }
  /** An item's events, newest first, and what was paid for it. */
  itemHistory(id: number): Observable<ItemHistory> {
    return this.http.get<ItemHistory>(`/api/items/${id}/history`);
  }
  /** Record what an item cost, for anything not bought through the Buy list. */
  recordPurchase(id: number, purchase: NewPurchase): Observable<Purchase> {
    return this.http.post<Purchase>(`/api/items/${id}/purchases`, purchase);
  }
  deletePurchase(itemId: number, purchaseId: number): Observable<unknown> {
    return this.http.delete(`/api/items/${itemId}/purchases/${purchaseId}`);
  }
  /** What is attached to an item, without the bytes (see `fileUrl`). */
  itemFiles(id: number): Observable<ItemFile[]> {
    return this.http.get<ItemFile[]>(`/api/items/${id}/files`);
  }

  /** The body is the file; the name and purchase ride in headers, so the name
   *  is stripped to ASCII. */
  addItemFile(id: number, file: File, purchaseId?: number): Observable<ItemFile> {
    const headers: Record<string, string> = {
      'Content-Type': file.type || 'application/octet-stream',
      'X-File-Name': file.name.replace(/[^\x20-\x7e]/g, '_').slice(0, 120) || 'attachment',
    };
    if (purchaseId != null) headers['X-Purchase-Id'] = String(purchaseId);
    return this.http.post<ItemFile>(`/api/items/${id}/files`, file, { headers });
  }

  deleteItemFile(id: number, fileId: number): Observable<unknown> {
    return this.http.delete(`/api/items/${id}/files/${fileId}`);
  }

  /** A plain link; the server answers it as a download. */
  fileUrl(id: number, fileId: number): string {
    return `/api/items/${id}/files/${fileId}`;
  }

  deleteLocation(id: number): Observable<unknown> {
    return this.http.delete(`/api/locations/${id}`);
  }

  house(): Observable<HouseScene> {
    return this.http.get<HouseScene>('/api/house');
  }

  /** Upcoming bin collections, soonest first; empty without a feed. */
  bins(): Observable<BinDay[]> {
    return this.http.get<BinDay[]>('/api/bins');
  }

  /** Put a shop trip in the Nextcloud calendar, with the Buy list in its
   *  description. 409: the calendar is not linked. */
  planShopTrip(shop: string, startsAt: string, items: string[]): Observable<PlannedTrip> {
    return this.http.post<PlannedTrip>('/api/calendar/shop-trip', {
      shop,
      starts_at: startsAt,
      items,
    });
  }

  shopping(): Observable<ShoppingItem[]> {
    return this.http.get<ShoppingItem[]>('/api/shopping');
  }
  /** Mark a row bought; the price is optional. */
  buyShopping(id: number, purchase?: { shop: string; amount_minor: number }): Observable<Item> {
    return this.http.post<Item>(`/api/shopping/${id}/buy`, purchase ? { purchase } : {});
  }

  lookupProduct(barcode: string): Observable<Product> {
    return this.http.get<Product>(`/api/products/${encodeURIComponent(barcode)}`);
  }
  searchProducts(q: string): Observable<Product[]> {
    return this.http.get<Product[]>('/api/products', { params: { q } });
  }
  /** A live Asda search, run by the server. */
  searchAsda(q: string): Observable<AsdaHit[]> {
    return this.http.get<AsdaHit[]>('/api/products/shop/asda', { params: { q } });
  }
  /** Does this shop carry this product's barcode? From memory when it can. */
  findAtShop(id: number, source: Source): Observable<ShopFind> {
    return this.http.get<ShopFind>(`/api/products/id/${id}/find/${encodeURIComponent(source)}`);
  }
  /** Report what this device saw at a shop the server cannot reach. */
  rememberShopListings(source: Source, listings: SeenListing[]): Observable<Remembered> {
    return this.http.post<Remembered>(
      `/api/products/shop/${encodeURIComponent(source)}/listings`,
      listings,
    );
  }
  /** Where each Buy row is known to be sold, from memory only. Empty means
   *  unknown, not nowhere. */
  shopCoverage(rows: CoverageQuery[]): Observable<RowCoverage[]> {
    return this.http.post<RowCoverage[]>('/api/shopping/coverage', rows);
  }
  /** `version` busts the cache after a replace. */
  productImageUrl(barcode: string, version?: number): string {
    const base = `/api/products/${encodeURIComponent(barcode)}/image`;
    return version ? `${base}?v=${version}` : base;
  }
  uploadProductImage(barcode: string, blob: Blob): Observable<void> {
    return this.http.put<void>(`/api/products/${encodeURIComponent(barcode)}/image`, blob, {
      headers: { 'Content-Type': blob.type },
    });
  }
  /** Import a shop's product into the catalogue. */
  importProduct(body: {
    source: Source;
    external_id: string;
    name: string;
    brand?: string | null;
    /** As the shop writes it ("400G"). */
    quantity_label?: string | null;
    /** Merges the shop's product with Open Food Facts' by barcode. */
    barcode?: string | null;
    image_url?: string | null;
    price?: PriceInput | null;
  }): Observable<Product> {
    return this.http.post<Product>('/api/products/import', body);
  }
  /** Attach or refresh a shop's listing, fetched and barcode-checked by the server. */
  syncListing(id: number, source: Source, externalId: string): Observable<Product> {
    return this.http.post<Product>(`/api/products/id/${id}/listings`, {
      source,
      external_id: externalId,
    });
  }
  /** Everything the product page shows. */
  getProductDetail(id: number): Observable<ProductDetail> {
    return this.http.get<ProductDetail>(`/api/products/id/${id}`);
  }
  /** Settle where sources disagree; returns the re-read detail. */
  reconcile(id: number, decisions: FieldChoice[]): Observable<ProductDetail> {
    return this.http.post<ProductDetail>(`/api/products/id/${id}/reconcile`, decisions);
  }
  /** Store a shop page's facts blob for the server to parse; `ean` is the page's
   *  own barcode, checked against the product. */
  submitFacts(
    id: number,
    body: { source: Source; ean: string; blob: string },
  ): Observable<ProductDetail> {
    return this.http.post<ProductDetail>(`/api/products/id/${id}/facts`, body);
  }
  /** For barcodeless products. */
  productImageByIdUrl(id: number, version?: number): string {
    const base = `/api/products/id/${id}/image`;
    return version ? `${base}?v=${version}` : base;
  }

  conflicts(): Observable<ConflictEntry[]> {
    return this.http.get<ConflictEntry[]>('/api/conflicts');
  }
  /** Values are JSON-encoded. */
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
  resolveConflict(id: number): Observable<void> {
    return this.http.post<void>(`/api/conflicts/${id}/resolve`, {});
  }

  trash(): Observable<TrashEntry[]> {
    return this.http.get<TrashEntry[]>('/api/trash');
  }
  /** `ref` is an id for REST kinds, a ulid for synced ones. */
  restoreTrash(kind: TrashKind, ref: string): Observable<void> {
    return this.http.post<void>(`/api/trash/${kind}/${encodeURIComponent(ref)}/restore`, {});
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
  /** One line per ingredient, including those nothing happened to. */
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
