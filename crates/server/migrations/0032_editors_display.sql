-- Display of each editor, as JSON (`EditorDisplay`, src/editor_display.rs):
-- its panels, open or closed with their size, and the tree of its object
-- navigator. Empty string as long as the editor has saved none: the page
-- then starts from its defaults. Kept while the editor is closed, so that
-- reopening it finds its display as it was left. It takes over the state of
-- the navigator (`editors.navigator`, migration 0031); the other panels of
-- such an editor start from the defaults of the page.
ALTER TABLE editors ADD COLUMN display TEXT NOT NULL DEFAULT '';

UPDATE editors SET display = json_object(
    'panels', json_object(
        'navigator', json_object(
            'open', CASE WHEN json_extract(navigator, '$.open') THEN json('true') ELSE json('false') END,
            'size', json_extract(navigator, '$.width')
        ),
        'explorer', json_object('open', json('false'), 'size', NULL),
        'editor', json_object('open', json('true'), 'size', NULL),
        'results', json_object('open', json('false'), 'size', NULL),
        'pivot', json_object('open', json('false'), 'size', NULL),
        'console', json_object('open', json('true'), 'size', NULL)
    ),
    'navigator', json_object(
        'expanded', json(json_extract(navigator, '$.expanded')),
        'scroll_top', json_extract(navigator, '$.scroll_top'),
        'tables_by_rows', json(COALESCE(json_extract(navigator, '$.tables_by_rows'), '[]'))
    )
)
WHERE navigator <> '';

ALTER TABLE editors DROP COLUMN navigator;
