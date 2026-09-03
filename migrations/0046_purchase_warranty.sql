-- Life schema, migration 0046: how long a purchase is covered for.

-- A warranty is a property of the TRANSACTION, not of the thing. "This
-- dishwasher has a two-year warranty" is not a fact anybody can act on: two
-- years from WHEN. The start date is the purchase, so the length lives beside
-- it, and "covered until" is derived from the two rather than stored — the same
-- discipline as `unit_amount_minor`, and for the same reason. A second stored
-- date can disagree with the purchase it came from; a derived one cannot.
--
-- On the item instead would have been the tempting place, because that is where
-- the question gets asked ("is this still covered?"). It is wrong for two
-- reasons. A replacement is a NEW purchase with its own shop, price and cover,
-- and an item-level field would have to be overwritten and lose the old one. And
-- an item that was never bought through the app has no date at all, so the field
-- would sit there holding a length that starts nowhere.
--
-- NULL means "no warranty recorded", which is not the same as "no warranty" —
-- most things in a cupboard have neither, and nothing should render a claim
-- about cover for a jar of oregano.

ALTER TABLE purchases ADD COLUMN IF NOT EXISTS warranty_months INT NULL;
