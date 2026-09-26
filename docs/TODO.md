# Life — what's next

Open work only; what is built is described in `docs/design/` and the code, and
how it got there is in `git log`. `#N` is a task in the `task` CLI.

## Features

- **House model** (#15) — the kitchen's oven tower, extractor hood and back
  door, then the other rooms. Built against the local preview; see
  `scenes/README.md` § "Live modelling workflow".
- **Cupboards in 3D** (#134) — place cupboards in scene coordinates so "where is
  my X" can highlight one. Blocked on the open decision below.
- **House polish** (#135) — camera/lighting, per-cupboard layers,
  tap-a-cupboard-to-list-its-items.
- **Low-stock suggestions** (#128) — derive from `ItemEvent::Low` (written when
  something goes on the Buy list), not from `used`, which nobody logs. The
  interval between `low` rows is the rhythm.
- **Meds: refill-soon** (#130) — needs how much is left and how fast it goes;
  same `low` signal as #128. Do not derive it from the printed expiry.
- **Purchases: derived views** — per-unit price ranking, and a like-for-like
  cheapest shop for a whole list (the Buy screen's per-shop estimates cover
  different rows, so they do not compare). Prices come from our own
  observations; Open Food Facts prices are at most a hint.
- **Unit conversion when consuming** — `src/inventory/consume.rs` matches units
  by string, so `1kg` of stock does not serve a `500g` line. `packsize.rs`
  already parses labels to g/ml/count.
- **Expiry view** — is a fuller view wanted beyond Today's "Expiring soon" card?
- **User-defined categories** — the category set is a closed enum, so a new kind
  needs a deploy. Not started until a kind is wanted that does not exist.
- **Product extras** — paste-URL → `og:image`; manual "refresh from OFF"; a
  `@zxing/browser` scanner fallback (`BarcodeDetector` is Chromium-only);
  contributing missing products to OFF (needs Pippijn's OFF account).

## Infrastructure

- **App-ingress NetworkPolicy** — held until k3s's kubelet probes are exempted.
- **Stale Nextcloud cookie in a plain browser** — "State token does not match"
  on sign-in. The Android app recovers by dropping NC's cookies; a browser user
  must clear them by hand.

## Parked

- **`noUncheckedIndexedAccess`** — surveyed: 59 sites, none a latent bug, 31 of
  them one golden-tested numeric kernel. Not worth the `!` assertions.

## Open decisions

- three.js parametric geometry vs an authored glTF model of the house.
- How scene cupboards relate to the DB location tree (store `position` on the
  `location` rows, vs keep scene geometry separate and map by id/name).
