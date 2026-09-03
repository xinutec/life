// Wire types shared with the Rust backend.
//
// The API DTOs are GENERATED from the Rust types by ts-rs (scripts/gen-types.sh
// → ./generated/) so they can't drift — do not hand-edit ./generated. A drift
// gate (scripts/check-types.sh, in the pre-push hook) fails if the Rust types
// change without regenerating.
export type { BinDay } from './generated/BinDay';
export type { PlannedTrip } from './generated/PlannedTrip';
export type { ConnectionStatus } from './generated/ConnectionStatus';
export type { Me } from './generated/Me';
export type { LocationKind } from './generated/LocationKind';
export type { Loc } from './generated/Loc';
export type { ItemCategory } from './generated/ItemCategory';
export type { Item } from './generated/Item';
export type { ItemEvent } from './generated/ItemEvent';
export type { ExpiryPrecision } from './generated/ExpiryPrecision';
export type { ItemHistory } from './generated/ItemHistory';
export type { ItemHistoryEntry } from './generated/ItemHistoryEntry';
export type { Purchase } from './generated/Purchase';
export type { ItemFile } from './generated/ItemFile';
export type { NewPurchase } from './generated/NewPurchase';
export type { Source } from './generated/Source';
export type { Product } from './generated/Product';
export type { PackSize } from './generated/PackSize';
export type { PackUnit } from './generated/PackUnit';
export type { ProductListing } from './generated/ProductListing';
export type { ProductDetail } from './generated/ProductDetail';
export type { SourceDocument } from './generated/SourceDocument';
export type { AsdaHit } from './generated/AsdaHit';
export type { ShopFind } from './generated/ShopFind';
export type { CoverageQuery } from './generated/CoverageQuery';
export type { Remembered } from './generated/Remembered';
export type { RowCoverage } from './generated/RowCoverage';
export type { CookedLine } from './generated/CookedLine';
export type { Take } from './generated/Take';
export type { Untouched } from './generated/Untouched';
export type { SeenListing } from './generated/SeenListing';
export type { TelemetryEvent } from './generated/TelemetryEvent';
export type { PriceInput } from './generated/PriceInput';
export type { ShopPrice } from './generated/ShopPrice';
export type { Choice } from './generated/Choice';
export type { ReconcileField } from './generated/ReconcileField';
export type { FieldChoice } from './generated/FieldChoice';
export type { ProductFacts } from './generated/ProductFacts';
export type { Nutrition } from './generated/Nutrition';
export type { Allergen } from './generated/Allergen';
export type { DietaryFlag } from './generated/DietaryFlag';
export type { Claim } from './generated/Claim';
export type { Presence } from './generated/Presence';
export type { ShoppingItem } from './generated/ShoppingItem';
export type { RecipeIngredient } from './generated/RecipeIngredient';
export type { Recipe } from './generated/Recipe';
export type { Todo } from './generated/Todo';
export type { TodoType } from './generated/TodoType';
export type { TodoStatus } from './generated/TodoStatus';
export type { TodoPriority } from './generated/TodoPriority';
export type { TodoLink } from './generated/TodoLink';
export type { LinkKind } from './generated/LinkKind';
export type { TargetKind } from './generated/TargetKind';
export type { ConflictEntry } from './generated/ConflictEntry';
export type { ConflictKind } from './generated/ConflictKind';
export type { TrashEntry } from './generated/TrashEntry';
export type { TrashKind } from './generated/TrashKind';
export type { SuggestEmotionsRequest } from './generated/SuggestEmotionsRequest';
export type { SuggestEmotionsResponse } from './generated/SuggestEmotionsResponse';
export type { EmotionCandidate } from './generated/EmotionCandidate';
export type { WarmEmotionsRequest } from './generated/WarmEmotionsRequest';

import type { ItemCategory as ItemCategoryT } from './generated/ItemCategory';
import { keysOf } from './shared/narrow';

/** Every ItemCategory, in the order category pickers show them. The `Record`
 *  keys make the compiler prove the list is exhaustive and duplicate-free — a
 *  new enum value won't build until it's placed here. */
// Ordered as a person would scan it, not alphabetically: the everyday ones
// first, `other` last because it is the fallback and a fallback offered early
// gets picked early — which is how `other` came to hold an avocado.
export const ITEM_CATEGORIES = keysOf({
  food: true,
  cookware: true,
  tableware: true,
  cleaning: true,
  clothing: true,
  appliance: true,
  tool: true,
  medication: true,
  document: true,
  other: true,
} satisfies Record<ItemCategoryT, true>);

/** What each category is CALLED, as opposed to what it is keyed by.
 *
 *  The pickers rendered the slug itself, which was survivable while the list was
 *  five short words and is not now: "tableware" beside "cookware" in lower case
 *  is two near-identical strings a person has to read letter by letter.
 *
 *  Typed as a total Record, so adding a variant is a compile error here — a
 *  category that reaches the UI unnamed would render as its slug and look like
 *  a bug in the data. */
export const ITEM_CATEGORY_LABEL: Record<ItemCategoryT, string> = {
  food: 'Food',
  cookware: 'Cookware (pans, trays)',
  tableware: 'Tableware (glasses, plates)',
  cleaning: 'Cleaning',
  clothing: 'Clothing',
  appliance: 'Appliance',
  tool: 'Tool',
  medication: 'Medication',
  document: 'Document',
  other: 'Other',
};

// Scene-file types are frontend-owned: /api/house streams scenes/house.json
// through as raw JSON (no Rust struct), so these aren't generated.

/** A furniture floor-box in the house scene. Centred at (cx,cz); w×d×h metres;
 *  y0 = base height off the floor. */
export interface Furniture {
  cx: number;
  cz: number;
  w: number;
  d: number;
  h: number;
  y0?: number;
  color?: string | null;
}

/** An opening (doorway / window / wide cased passage) cut into one of a room's
 *  walls (`wall` = index into that room's `walls`), leaving a lintel above (and a
 *  sill below, for windows). `offset` is metres from the wall's start to the near
 *  edge; `width`×`height` size the hole; `sill` lifts the bottom off the floor
 *  (0/omitted = floor-level). A doorway between two rooms is just an opening in
 *  each room's copy of the shared wall. `depth`/`leads` are informational. */
export interface WallOpening {
  wall: number;
  offset: number;
  width: number;
  height: number;
  sill?: number;
  depth?: number;
  leads?: string;
}

/** One room: its own closed outline, walked turtle-style from `start` (world XZ
 *  of the first corner) at `heading` degrees — each wall is [turn_deg, length_m].
 *  Rooms that adjoin simply repeat the shared wall in each of their outlines. */
export interface Room {
  name?: string;
  start: [number, number];
  heading?: number;
  walls: [number, number][];
  openings?: WallOpening[];
}

/** Hand-authored house geometry (scenes/house.json): a set of rooms (each its own
 *  outline) plus furniture. See scenes/README.md. */
export interface HouseScene {
  height: number;
  rooms: Room[];
  furniture: Furniture[];
  highlight?: number | null;
  question?: string;
}
