-- Configuration of the Pivoteur of each result tab, as JSON (`TabPivot`).
ALTER TABLE result_tabs ADD COLUMN pivot TEXT NOT NULL DEFAULT '{}';
