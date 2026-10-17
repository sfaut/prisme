-- How CAST reads a string as a number: a column of a table, then the same strings as
-- literals, to SIGNED, UNSIGNED, DECIMAL and DOUBLE (MySQL from 8.0.17). The spaces
-- before the number, the sign, the fraction, the exponent, the bounds of the integers.
DROP TABLE IF EXISTS cn;
CREATE TABLE cn (id INT PRIMARY KEY, s VARCHAR(40) NOT NULL);
INSERT INTO cn VALUES (1, '12'), (2, '  12'), (3, CONCAT(CHAR(9 USING utf8mb4), '12')), (4, CONCAT(CHAR(10 USING utf8mb4), '12')), (5, CONCAT(CHAR(11 USING utf8mb4), '12')), (6, CONCAT(CHAR(12 USING utf8mb4), '12')), (7, CONCAT(CHAR(13 USING utf8mb4), '12')), (8, CONCAT(' ', CHAR(10 USING utf8mb4), '12')), (9, CONCAT(_utf8mb4 X'C2A0', '12')), (10, '12  '), (11, CONCAT('12', CHAR(10 USING utf8mb4))), (12, '+12'), (13, '-12'), (14, '- 12'), (15, '--12'), (16, '0012'), (17, '1.9'), (18, '-1.9'), (19, '.5'), (20, '5.'), (21, '1e3'), (22, '1.5e1'), (23, '-2.5e0'), (24, '0x10'), (25, '12abc'), (26, 'abc'), (27, ''), (28, '1,5'), (29, '9223372036854775807'), (30, '9223372036854775808'), (31, '18446744073709551615'), (32, '18446744073709551616'), (33, '-9223372036854775809'), (34, '99999999999999999999999'), (35, '-0'), (36, '2.5'), (37, '-2.5'), (38, '1e400'), (39, '-1e400'), (40, '1e-400'), (41, '１２'), (42, '12.345678901234567890123456789012345'), (43, '0.5'), (44, '-0.5');
SELECT id, HEX(s) AS hex_, CAST(s AS SIGNED) AS signed_, CAST(s AS UNSIGNED) AS unsigned_, CAST(s AS DECIMAL(30, 2)) AS decimal_, CAST(s AS DECIMAL) AS decimal_default FROM cn ORDER BY id;
SELECT id, CAST(s AS DOUBLE) AS double_ FROM cn ORDER BY id;
SELECT id, CAST(s AS FLOAT) AS float_ FROM cn WHERE id NOT IN (38, 39) ORDER BY id;
SELECT CAST('1e400' AS FLOAT) AS over_;
SELECT CAST(_utf8mb4 X'0A3132' AS SIGNED) AS lf, CAST(_utf8mb4 X'0B3132' AS SIGNED) AS vt, CAST(_utf8mb4 X'093132' AS SIGNED) AS tab, CAST(_utf8mb4 X'C2A03132' AS SIGNED) AS nbsp, CAST('1e3' AS SIGNED) AS exp_, CAST('-1.9' AS SIGNED) AS neg_frac, CAST('18446744073709551615' AS SIGNED) AS ubig, CAST('18446744073709551616' AS SIGNED) AS over_, CAST('-9223372036854775809' AS SIGNED) AS under;
SELECT CAST(_utf8mb4 X'0A3132' AS UNSIGNED) AS lf, CAST(_utf8mb4 X'0A3132' AS DECIMAL(10, 2)) AS lf_decimal, CAST('-12' AS UNSIGNED) AS neg, CAST('-1.9' AS UNSIGNED) AS neg_frac, CAST('99999999999999999999999' AS UNSIGNED) AS huge, CAST('-0.5' AS UNSIGNED) AS neg_half;
SELECT CAST(_utf8mb4 X'0A3132' AS DOUBLE) AS lf, CAST('1e400' AS DOUBLE) AS over_, CAST('0x10' AS DOUBLE) AS hex_;
