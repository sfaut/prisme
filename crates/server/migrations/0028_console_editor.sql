-- Editor whose session wrote each element of the console: the console of an
-- editor shows its own elements, all of them on request.
--
-- editor_id: id of the editor, 0 outside an editor (navigator, export,
-- connection test, the server itself, notifications of the interface); an
-- integer and no foreign key, so that the console outlives the editor.
ALTER TABLE query_history ADD COLUMN editor_id INTEGER NOT NULL DEFAULT 0 CHECK (editor_id >= 0);
ALTER TABLE debug_log ADD COLUMN editor_id INTEGER NOT NULL DEFAULT 0 CHECK (editor_id >= 0);

CREATE INDEX query_history_editor ON query_history (user_id, editor_id, id);
CREATE INDEX debug_log_editor ON debug_log (editor_id, id);
