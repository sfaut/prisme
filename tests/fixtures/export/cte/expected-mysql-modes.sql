WITH ds (id, prix, taux, "créé le", horodatage, jour, heure, "date zéro", "select", vide, json, octets, drapeau, genre, année) AS (
    VALUES
        ROW (1, 12.50, 0.125E0, TIMESTAMP'2026-10-12 09:32:00', TIMESTAMP'2026-10-12 09:32:00.123456', DATE'2026-10-12', TIME'12:00:00', '0000-00-00', 'dit ''oui''', '', CAST('{"a": "x\ny"}' AS JSON), X'FF001A', b'101', 'M', 2026),
        ROW (-2, -3.00, 0.00000015E0, NULL, TIMESTAMP'2026-01-01 00:00:00.000000', DATE'0000-01-01', TIME'-838:59:59', '2026-10-00', 'a\b \n', NULL, CAST('[1, 2]' AS JSON), _binary'abc', b'0', 'F', 1901),
        ROW (3, 1000000.00, -0.5E0, TIMESTAMP'2026-02-03 04:05:06', NULL, NULL, TIME'00:00:01', DATE'2026-10-12', 'ligne 1
ligne 2	tab', ' espace ', CAST('{"b": null}' AS JSON), _binary'', b'11111111', 'M', NULL),
        ROW (4, NULL, 1e20, TIMESTAMP'2026-12-31 23:59:59', TIMESTAMP'2026-12-31 23:59:59.999999', DATE'2026-02-28', NULL, NULL, '€ 😀 ''''', 'x', NULL, NULL, NULL, NULL, 2155)
)
SELECT ALL id, prix, taux, "créé le", horodatage, jour, heure, "date zéro", "select", vide, json, octets, drapeau, genre, année
FROM ds
;
