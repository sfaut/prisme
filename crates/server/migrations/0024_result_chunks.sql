-- Rows of the result tabs, in chunks written as they arrive (src/recorder.rs):
-- the rows of a result of millions of rows never make one value, which SQLite
-- caps at 1,000,000,000 bytes, nor one answer, which the page could not read
-- beyond 536,870,888 characters (docs/QUESTIONS.md, question 32).
--
-- result_tabs.run: run of the tab whose chunks make its saved result. A
-- refresh writes the chunks of the next run, which only replace them once it
-- succeeds.
-- run: run of the tab that wrote the chunk.
-- position: rank of the chunk in its run, from 0.
-- rows: rows of the chunk, in JSON: an array of rows, each one an array of
-- texts or nulls.
ALTER TABLE result_tabs ADD COLUMN run INTEGER NOT NULL DEFAULT 0;

CREATE TABLE result_chunks (
    tab_id INTEGER NOT NULL REFERENCES result_tabs (id) ON DELETE CASCADE,
    run INTEGER NOT NULL,
    position INTEGER NOT NULL,
    rows TEXT NOT NULL,
    PRIMARY KEY (tab_id, run, position)
) STRICT;

-- The rows already saved, one value per tab, become chunks of 10,000 rows.
INSERT INTO result_chunks (tab_id, run, position, rows)
SELECT result_tabs.id, 0, saved.key / 10000, json_group_array(json(saved.value) ORDER BY saved.key)
FROM result_tabs, json_each(result_tabs.rows) AS saved
GROUP BY result_tabs.id, saved.key / 10000;

ALTER TABLE result_tabs DROP COLUMN rows;
