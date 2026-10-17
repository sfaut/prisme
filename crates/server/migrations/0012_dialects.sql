-- Dialect the user writes in (docs/DEV-DIALECTS.md, step 4): `source` for the
-- native SQL of the data source, or a dialect with its version, such as
-- `mysql 8.0.19`, checked by the server against the registry of the versions.
--
-- users.preferred_dialect: setting of the profile ("Dialecte préféré").
-- editors.dialect: choice of the editor tab; the empty string stands for the
-- setting of the profile.
ALTER TABLE users ADD COLUMN preferred_dialect TEXT NOT NULL DEFAULT 'source' CHECK (length(preferred_dialect) <= 30);
ALTER TABLE editors ADD COLUMN dialect TEXT NOT NULL DEFAULT '' CHECK (length(dialect) <= 30);
