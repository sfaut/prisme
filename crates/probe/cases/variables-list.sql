-- System variables of each server, with their scope and their type: the
-- tables of `performance_schema` in MySQL (`G` global, `S` session, `M`
-- metadata), `information_schema.SYSTEM_VARIABLES` in MariaDB (`I`).
SELECT CONCAT('G ', VARIABLE_NAME) AS v FROM performance_schema.global_variables ORDER BY VARIABLE_NAME;
SELECT CONCAT('S ', VARIABLE_NAME) AS v FROM performance_schema.session_variables ORDER BY VARIABLE_NAME;
SELECT CONCAT('M ', VARIABLE_NAME, ' ', VARIABLE_SCOPE, ' ', DATA_TYPE) AS v FROM performance_schema.variables_metadata ORDER BY VARIABLE_NAME;
SELECT CONCAT('I ', VARIABLE_NAME, ' ', VARIABLE_SCOPE, ' ', VARIABLE_TYPE) AS v FROM information_schema.SYSTEM_VARIABLES ORDER BY VARIABLE_NAME;
