-- Display of the result grid of each tab, as JSON (`GridDisplay`,
-- src/grid_display.rs): widths and order of its columns, sort, user filter,
-- scroll. Empty string as long as the tab has saved none: the grid then
-- starts from its defaults. A new run carries it over to the tab that
-- replaces this one, as the configuration of the Pivoteur.
ALTER TABLE result_tabs ADD COLUMN grid TEXT NOT NULL DEFAULT '';
