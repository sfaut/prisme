-- Collation of the server and of the connection, and the default collation
-- of each character set (`character_set_collations`, MariaDB from 11.2),
-- which a string after an introducer takes.
SELECT @@version, @@character_set_server, @@collation_server, @@character_set_collations, @@collation_connection;
