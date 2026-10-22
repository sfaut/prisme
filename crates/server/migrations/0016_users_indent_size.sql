-- Indentation of the SQL editor, in spaces, set in the settings: gap between
-- two tab stops (Tab, Shift+Tab, Backspace in the indentation).
ALTER TABLE users ADD COLUMN indent_size INTEGER NOT NULL DEFAULT 4 CHECK (indent_size BETWEEN 1 AND 8);
