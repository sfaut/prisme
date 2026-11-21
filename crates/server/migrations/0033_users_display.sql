-- Theme of the interface chosen by the user: following the machine
-- (`system`), light or dark. The page keeps a copy of it in the browser, to
-- show the right theme before it reads this one.
ALTER TABLE users ADD COLUMN theme TEXT NOT NULL DEFAULT 'system' CHECK (theme IN ('system', 'light', 'dark'));

-- Display settings of the user, as JSON (`UserDisplay`, src/user_display.rs,
-- without its theme): the active editor, the tab and the filters of the
-- console, the wrapping of the data explorer. Empty string as long as the
-- user has saved none: the defaults apply.
ALTER TABLE users ADD COLUMN display TEXT NOT NULL DEFAULT '';
