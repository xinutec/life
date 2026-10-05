-- Life schema, migration 0051: a picture always says where it came from.
--
-- Pictures stored before `image_source` existed came from Open Food Facts, the
-- only source that fetched them then. A hand upload among them would be
-- relabelled too; that was judged not worth keeping apart. Prod had none left
-- unlabelled when this was written, and no source without a picture.

UPDATE products SET image_source = 'off' WHERE image IS NOT NULL AND image_source IS NULL;
UPDATE products SET image_source = NULL WHERE image IS NULL;

ALTER TABLE products ADD CONSTRAINT picture_has_a_source
    CHECK ((image IS NULL) = (image_source IS NULL));
