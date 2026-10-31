-- Read-only account of the development servers (`compose.yaml`), created on
-- their first start: `reader`, without a password, reads every database and
-- writes nothing.

CREATE USER reader@'%';
GRANT SELECT, SHOW VIEW ON *.* TO reader@'%';
