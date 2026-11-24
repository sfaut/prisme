WITH ds (id, prix, taux, "créé le", jour, heure, "select", vide, json, octets) AS (
    VALUES
        (1, 12.5, 0.125E0, '2026-10-12 09:32:00', '2026-10-12', '12:00:00', 'dit ''oui''', '', '{"a": "x\ny"}', X'FF001A'),
        (-2, -3, 0.00000015E0, NULL, '2026-01-01', '23:59:59.999', 'a\b \n', NULL, '[1, 2]', X'616263'),
        (3, 1000000, -0.5E0, '2026-02-03 04:05:06', NULL, '00:00:01', 'ligne 1
ligne 2	tab', ' espace ', '{"b": null}', X''),
        (4, NULL, 100000000000000000000E0, '2026-12-31T23:59:59Z', '2026-02-28', NULL, '€ 😀 ''''', 'x', NULL, NULL)
)
SELECT ALL id, prix, taux, "créé le", jour, heure, "select", vide, json, octets
FROM ds
;
