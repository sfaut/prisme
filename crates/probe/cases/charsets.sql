-- Character sets of each server, their default collation and their largest
-- character in bytes: the list of the introducers (`_latin1'a'`).
SELECT CHARACTER_SET_NAME, DEFAULT_COLLATE_NAME, MAXLEN FROM information_schema.CHARACTER_SETS ORDER BY 1;
