-- The saved result of a tab reports what its statement did in a neutral
-- form, `outcome`, filled by the connector of each engine: the number of
-- affected rows moves from `affected_rows` to `outcome.affected`. The verb of
-- the statement, which needs its AST, stays at its default for the results
-- saved before.
UPDATE result_tabs
SET result = json_set(
    json_remove(result, '$.affected_rows'),
    '$.outcome',
    json_object('affected', json_extract(result, '$.affected_rows'))
)
WHERE json_type(result, '$.affected_rows') = 'integer';
