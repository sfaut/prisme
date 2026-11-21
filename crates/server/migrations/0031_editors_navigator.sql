-- State of the object navigator of each editor, as JSON (`NavigatorState`,
-- src/navigator_state.rs): panel open and its width, open nodes, scroll.
-- Empty string as long as the editor has saved none: the page then starts
-- from its defaults. Kept while the editor is closed, so that reopening it
-- finds its navigator as it was left.
ALTER TABLE editors ADD COLUMN navigator TEXT NOT NULL DEFAULT '';
