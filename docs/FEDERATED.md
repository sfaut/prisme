# Prisme : le moteur fédéré (après la v1)

Ce document décrit la vision de départ du projet, qui vient après la v1. L'idée de départ : un client qui interroge **plusieurs sources à la fois** (serveurs SQL, fichiers CSV) comme s'il n'y en avait qu'une, avec un dialecte neutre. Les mentions **[à confirmer]** signalent une proposition non validée.

La v1 (voir `docs/V1.md`) construit le noyau commun : parseur, AST, émetteurs par dialecte, connecteurs, streaming, annulation. Le moteur fédéré s'y ajoute.

## 1. Besoin

- Interroger MySQL, MariaDB et des fichiers CSV (puis d'autres sources) dans une même requête, de 3 à 6 sources possibles.
- Volumes : des dizaines de millions de lignes, parfois des centaines de millions pour le détail. Les clés de jointure sont indexées dans les bases.
- **Motivation principale : la performance.** Sébastien juge DuckDB mauvais dès qu'on croise plusieurs sources : il rapatrie trop de données. Le moteur doit analyser la requête, chercher les données de façon optimale dans chaque source, puis assembler. Il y a donc du travail côté serveur et côté client.
- Objectif d'optimisation : faire transiter le moins de données possible et limiter le travail local.
- Le résultat doit être **cohérent, constant, reproductible**, et vu par l'utilisateur comme si la source était unique : types harmonisés (entiers, chaînes, booléens), comme pour une base unique.
- Test de vérité à faire tôt : « CSV joint à MySQL, avec filtre », comparé au temps de DuckDB.

## 2. Langages d'entrée

- Dialecte neutre, **ANSI strict comme noyau**, enrichi au besoin du meilleur des autres moteurs (DuckDB, ClickHouse). Exemple cité : les fonctions donnant le premier et le dernier jour d'une semaine. La requête enrichie est ensuite compilée vers le dialecte de la source visée.
- Autres langages possibles, qui se mappent bien sur le plan logique parce qu'ils sont des pipelines d'opérations relationnelles :
  - **Syntaxe SQL en pipe** (`FROM t |> WHERE ... |> AGGREGATE ...`). Sébastien a écrit « PSQL » ; c'est une interprétation de ma part, non confirmée. **[à confirmer]**
  - **PRQL** : chaque étape correspond presque un pour un à un nœud de plan. Il a une notion de dialecte cible à la compilation. Fixer un sous-ensemble, car la spécification évolue.
  - **Tutorial D** (voir la section 5).
- **Un langage par requête**, choisi par l'utilisateur selon son aisance. Pas de mélange dans une même requête. Le choix du langage n'a plus d'effet après le parsing.
- Ordre suggéré : SQL enrichi, puis syntaxe pipe (peu coûteuse, valide que le plan ne dépend pas de la forme du SQL), puis PRQL (vrai changement de paradigme).
- Un langage orienté documents, avec structures imbriquées, ne rentre pas dans l'algèbre relationnelle sans aplatir les données ou gérer des types imbriqués, ce qui touche le modèle de types.

## 3. Architecture

Pipeline commun, où **le plan logique est le pivot** et ne dépend d'aucun langage :

1. **Frontend** (choisi par l'utilisateur) : texte vers plan logique.
2. **Analyse** : résolution des tables et colonnes, types, catalogue de fonctions, classification lecture / écriture / DDL.
3. **Optimiseur** : écrit une seule fois, travaille uniquement sur le plan logique.
4. **Plan physique** : pour chaque jointure, semi-join, merge join ou hash join local, selon les index et les statistiques.
5. **Exécution** : sous-requêtes envoyées en parallèle, résultats en streaming, assemblage local minimal.
6. **Émetteurs** : un par source, qui traduit le plan (ou un sous-arbre poussé) dans le dialecte de la source.

Une commande **EXPLAIN** est à prévoir dès le début : plan logique optimisé, SQL généré pour chaque source, ce qui reste en local. C'est le meilleur outil de débogage et d'explication pour l'utilisateur.

## 4. Optimiseur

**Règles de réécriture**

- **Pushdown** des filtres, projections, agrégats et limites vers la source.
- **Agrégation avant jointure** (*eager aggregation*) : pré-agréger côté source en groupant sur les clés de jointure. Valide pour `SUM`, `COUNT`, `MIN`, `MAX`, et `AVG` via `SUM` et `COUNT` (le `COUNT` doit porter sur la colonne, pas sur `*`). **Invalide** pour `COUNT(DISTINCT)` et quand la jointure ou un `Ungroup` multiplie les lignes sans correction. Pas toujours rentable : décision par les statistiques.
- **Réordonnancement des jointures** : commencer par celle qui réduit le plus le volume.
- **Regroupement par source** : un sous-arbre qui ne touche qu'une source part entier, en une seule requête.
- **Réécriture de fonctions en plages** pour garder l'usage des index : `WHERE date_trunc('week', col) = X` devient `col >= début AND col < fin`. Les fonctions volatiles comme `current_date` sont évaluées une seule fois par Prisme.

**Stratégies de jointure entre sources**

- **Semi-join par lots** : récupérer les clés de la petite partie, puis interroger l'autre source par paquets (`IN` de quelques milliers de valeurs). Chaque lot utilise l'index de la clé. Variante : filtre de Bloom.
- **Merge join en streaming** : si la clé est indexée des deux côtés, demander `ORDER BY clé` aux deux sources en lecture non bufferisée et en parallèle, puis fusionner les deux flux en mémoire quasi constante. L'ordre renvoyé par les sources doit être identique à l'ordre de comparaison de Prisme (collations, position des NULL), sinon des lignes sautent silencieusement.
- **Hash join local**, uniquement sur des résultats déjà réduits, avec plafond mémoire et repli sur disque (fichiers temporaires triés) au-delà.
- Règle d'or à cette échelle : **ne jamais rapatrier de lignes brutes**. Chaque source réduit son volume avant que Prisme n'y touche.
- Résultats toujours en streaming vers le client (pages ou curseur), sans jamais tout matérialiser en mémoire.

**Statistiques**

- Un `COUNT(*)` sur des centaines de millions de lignes est lent : utiliser `information_schema` (estimations) et la cardinalité des index.
- Taille d'un CSV connue, nombre de lignes estimé par échantillonnage.
- Les statistiques doivent compter les NULL (taux par colonne) pour estimer la sélectivité.

**Stratégie de développement**

- v1 de l'optimiseur : règles fixes (filtres, projections, regroupement par source), puis agrégation avant jointure quand elle est valide, puis semi-join. Le merge join vient ensuite, puis le choix fondé sur les coûts.
- Briques existantes à connaître : Apache DataFusion (Rust) et Calcite (Java) ont parseur, optimiseur et règles, y compris l'agrégation poussée sous les jointures ; Trino fait tout cela mais est lourd. Sébastien veut éviter les dépendances, donc le modèle de leur plan logique sert de référence. DataFusion reste une option pour l'exécuteur le moment venu.
- Le gros du travail n'est pas l'analyse du texte (quelques millisecondes) mais le transfert et le traitement des lignes.

## 5. Plan logique et Tutorial D

### 5.1 Nœuds du plan

Les nœuds portent un **schéma de sortie** (colonnes, type, nullabilité), une estimation de lignes et, après optimisation, la source d'exécution (ou `local`). Le mode de doublons est **explicite dans le nom**, sans drapeau à interpréter (demande de Sébastien).

- **Source** : `Scan` (source, table ou fichier, colonnes lues, options CSV, filtre et limite poussés).
- **Unaires** : `Filter` (prédicat booléen, logique à trois valeurs), `Project`, `Extend` (colonnes calculées), `Rename`, `Sort` (clés, sens, position des NULL ; un tri n'est jamais supposé conservé après un autre nœud), `Limit`, `Distinct` (NULL considérés égaux).
- **Jointures** : `InnerJoin`, `LeftJoin`, `RightJoin`, `FullJoin`, `SemiJoin` (MATCHING) et `AntiJoin` (NOT MATCHING). Le schéma de sortie d'un `SemiJoin` ou d'un `AntiJoin` est celui du côté gauche. Ils ne multiplient jamais les lignes. Une clé NULL ne correspond jamais.
- **Ensembles** :

| Nœud | Équivalent SQL | Doublons |
|---|---|---|
| `UnionAll` | `UNION ALL` | conservés (somme des occurrences) |
| `UnionDistinct` | `UNION` | éliminés |
| `MinusAll` | `EXCEPT ALL` | max(0, n − m) occurrences |
| `MinusDistinct` | `EXCEPT` | éliminés |
| `IntersectAll` | `INTERSECT ALL` | min(n, m) occurrences |
| `IntersectDistinct` | `INTERSECT` | éliminés |

- **Agrégation et imbrication** : `Aggregate` (clés et fonctions, `DISTINCT` éventuel ; les NULL forment un seul groupe), `Group`, `Ungroup` (multiplie les lignes), `Wrap`, `Unwrap`.
- **Écriture**, toujours à la racine et vers une seule cible : `Insert`, `Update`, `Delete`, `Export`. La classification lecture / écriture se lit sur la racine.

**Invariants**

- Le plan est un arbre, sans cycle.
- Une réécriture ne change jamais le schéma de sortie de la racine.
- Un nœud poussé vers une source ne contient que des fonctions marquées « poussables » pour cette source.
- Les propriétés dérivées (nullabilité, unicité de clé, ordre) se calculent sur le plan, jamais sur le texte d'origine.

Dans les langages d'entrée : en SQL, `UNION` seul reste accepté comme en SQL standard (= `UnionDistinct`), avec les formes explicites `UNION DISTINCT` et `EXCEPT DISTINCT` en plus. Côté sources, `MinusAll` et `IntersectAll` n'existent pas partout : le catalogue dit par source si le nœud est poussable, sinon Prisme l'exécute en local (comptage par table de hachage).

### 5.2 Tutorial D comme vocabulaire

Sébastien préfère s'en tenir à la **théorie de C. J. Date**. Tutorial D (langage d'enseignement du *Third Manifesto* de Date et Darwen) fournit le vocabulaire du plan logique : `JOIN`, `UNION`, `MINUS`, `WHERE`, projection, `RENAME`, `EXTEND`, `SUMMARIZE`, `GROUP` / `UNGROUP` (attributs à valeur relation), `WRAP` / `UNWRAP` (attributs à valeur tuple), `MATCHING` et `NOT MATCHING` (le semi-join est un opérateur de première classe), `TCLOSE` (fermeture transitive). Sa grammaire est petite et régulière, donc facile à parser. Implémentations de référence : Rel (Java) et Dee (Python).

Dans le plan, `Group` et `Ungroup` sont de vrais nœuds, et non des fonctions opaques (`array_agg`, `UNNEST`) : l'optimiseur peut raisonner dessus. La relation imbriquée reste une relation (fermeture), donc pas de monde parallèle pour les valeurs imbriquées.

### 5.3 Mode flex et mode strict

Deux décisions imposées par la pureté de Date :

- **Ensembles ou sacs** : Date impose des relations sans doublons, les sources SQL renvoient des sacs. **Décision : sacs en interne**, pour rester fidèle aux sources et éviter un `DISTINCT` à l'entrée sur des centaines de millions de lignes.
- **NULL** : Date les rejette. Les sources en sont pleines, et convertir un NULL en 0 ou en -1 déforme le sens des données. **Décision : on conserve les NULL.**

Sébastien aimerait la pureté, mais les sources sont trop hétérogènes. D'où deux modes, choisis par requête :

- **Flex** : dialecte dérivé de Tutorial D, aligné sur la sémantique SQL des sources (logique à trois valeurs, sacs, attributs nullables comme `INTEGER?`, opérateurs `IS NULL`, `COALESCE`, `NULLIF`). `UNION ALL` et `MINUS ALL` sont des extensions du flex. À documenter précisément pour ne pas induire en erreur ceux qui connaissent l'original.
- **Strict** : simple **couche de vérification** sur le même plan. L'analyseur rejette la requête si un attribut peut être NULL ou si le résultat peut avoir des doublons, sauf traitement explicite (`COALESCE`, `DISTINCT`, colonne déclarée non nulle). Seules les formes `DISTINCT` des opérateurs ensemblistes y sont acceptées. L'exécution est identique.
- **Le mode strict est reporté** (non critique). Ce qui est critique dès maintenant : réserver l'information de **nullabilité** dans les types et dans le plan.

## 6. Modèle de types

- Types internes : entier, décimal exact, chaîne, booléen, date/heure, NULL, avec nullabilité. À fixer en premier, car tout en dépend.
- **Entiers** : un `BIGINT UNSIGNED` MySQL dépasse l'entier signé 64 bits. Les entiers hors plage sont lus en chaîne, puis comparés et calculés avec une bibliothèque de grands entiers si nécessaire. Un calcul qui déborde lève une **erreur explicite**, sans passer silencieusement en flottant. Le pushdown règle souvent le problème : si la somme se fait côté MySQL, il ne reste qu'un résultat à transporter.
- **Décimaux** : exacts, jamais en flottant.
- **Chaînes** : `'a' = 'A'` dépend de la collation de la source. Une règle unique est à fixer. **[à décider]**
- **Dates** : une seule convention de fuseau pour tous. **[à décider]**
- **Valeurs imbriquées** : réserver `LIST` et `STRUCT` dans le modèle dès le départ. Sébastien a posé la question des MVA (multi-value attributes). Précision : le modèle relationnel strict (1NF classique) interdit les valeurs multiples, mais Date soutient que « atomique » dépend des opérateurs fournis et défend les attributs à valeur relation (RVA), compatibles avec le modèle relationnel. Un tableau SQL (ordonné, avec doublons) n'est pas un RVA (ensemble sans ordre ni doublons), ce qui compte pour l'équivalence des réécritures. Le « MVA » de Manticore et Sphinx est en réalité une liste d'entiers. v1 du moteur : liste (plus familière aux utilisateurs de DuckDB et ClickHouse), en gardant la porte ouverte aux relations imbriquées. Minimum à implémenter : `array_agg`, `UNNEST` et le test d'appartenance.
- MySQL et MariaDB n'ont pas de vrai type tableau : `array_agg` devient `JSON_ARRAYAGG`, `UNNEST` devient `JSON_TABLE`, `list_contains` devient `MEMBER OF` ou `JSON_CONTAINS`.

## 7. NULL : liste des points à trancher

Référence en mode flex : le comportement des sources SQL. **Priorité** : les points 1 à 6 sont indispensables dès le départ (ils touchent la justesse des résultats et des jointures, et donc déjà la v1), les points 7 à 9 se décident avec le catalogue de fonctions, les points 10 à 12 avec le workspace et l'optimiseur. Aucun de ces points n'est encore tranché.

1. **Égalité** : égalité de comparaison (`=`, jointures : `NULL = NULL` donne inconnu) et égalité d'identité (« non distincts », `IS NOT DISTINCT FROM`, `<=>` en MySQL). Nommer les deux dans le plan et dire laquelle chaque nœud utilise.
2. **Logique à trois valeurs** : tables de vérité de `AND`, `OR`, `NOT` ; `Filter` rejette l'inconnu ; `CHECK` accepte l'inconnu ; `CASE WHEN` inconnu n'est pas vrai.
3. **Jointures** : clé NULL ne correspond jamais (y compris `SemiJoin`). `AntiJoin` conserve une clé NULL à gauche. Piège : `NOT IN` avec un NULL dans la sous-requête ne renvoie rien. Le frontend SQL doit-il le reproduire tel quel ou le traduire en `AntiJoin` ? Les deux ne sont pas équivalents. Option : jointure sur égalité d'identité.
4. **Opérateurs ensemblistes** : doublons selon l'identité, NULL compris, pour `Distinct`, `Union*`, `Minus*`, `Intersect*`. Vérifier que les sources suivent cette règle, y compris pour `EXCEPT ALL`.
5. **Agrégation** : `GROUP BY` regroupe tous les NULL ; les fonctions ignorent les NULL sauf `COUNT(*)` ; agrégat sur un ensemble vide ou tout NULL : `SUM`, `MIN`, `MAX`, `AVG` donnent NULL, `COUNT` donne 0 ; `COUNT(DISTINCT)` ignore les NULL ; `array_agg` garde-t-il les NULL ? (varie selon les moteurs, à fixer).
6. **Tri** : position des NULL (MySQL et MariaDB en premier en ordre croissant, PostgreSQL et DuckDB en dernier). Une règle de référence pour Prisme, et un `NULLS FIRST` / `NULLS LAST` explicite émis vers les sources, émulé si besoin. Crucial pour le merge join.
7. **Fonctions** : un argument NULL donne NULL, sauf exceptions à lister (`COALESCE`, `NULLIF`, `IS NULL`, `CONCAT` contre `CONCAT_WS` contre `||`). Division par zéro : NULL ou erreur ? Casts ratés : erreur, ou NULL via une variante `TRY_CAST` ?
8. **Types et nullabilité** : propagation par chaque nœud (`LeftJoin` rend nullables les colonnes de droite, `Aggregate` sans groupe peut produire un NULL, `UnionAll` est nullable si un côté l'est). Un `Filter` sur `IS NOT NULL` rend la colonne non nulle dans la suite du plan.
9. **Imbrication** : liste NULL et liste vide sont différentes. `Ungroup` d'une liste NULL ou vide : zéro ligne, ou une ligne avec NULL (`UNNEST` contre `LEFT JOIN UNNEST`) ? Élément NULL dans une liste : conservé ?
10. **CSV et workspace** : marqueurs de NULL à la lecture (voir la section 8) et à l'écriture (vide, `\N`, `NA`). SQLite autorise plusieurs NULL dans une colonne `UNIQUE`.
11. **Optimiseur** : chaque règle préserve la sémantique des NULL. Pousser un filtre sous un `LeftJoin` n'est valide que du bon côté ; transformer un `LeftJoin` en `InnerJoin` exige un filtre qui rejette les NULL de droite ; `NOT (a = b)` n'est pas `a IS DISTINCT FROM b`.
12. **Mode strict** : reporté, mais la nullabilité du point 8 doit exister.

Absence d'`ORDER BY` : l'ordre n'est pas garanti, comme dans n'importe quelle base. **Décision de Sébastien : le moteur n'ajoute pas de tri déterministe d'office.**

## 8. CSV

Principe : **tout est texte**, sans inférence. Sébastien préfère s'en tenir aux `CAST` écrits dans la requête plutôt qu'à une phase de modélisation (un schéma par fichier). Prévoir tout de même la déclaration de source.

**Déclaration de source** (comme une connexion MySQL) : chemin (dans un dossier **autorisé par rôle**, pour empêcher `../../etc/...`), séparateur (`,` par défaut, réglable, pas d'auto-détection en v1), encodage (UTF-8 par défaut, BOM toléré, autres par option), en-tête (présent par défaut, sinon colonnes `c1`, `c2`... ; un en-tête en doublon est une erreur), guillemets `"` avec guillemet doublé (RFC 4180), fins de ligne CRLF, LF ou CR.

**Valeurs**

- Aucun NULL par défaut : une cellule vide donne une chaîne vide.
- Option sur le **scan** : `WITH EMPTY AS NULL`, et la forme générale `WITH NULL AS 'NA'` (`EMPTY AS NULL` est un raccourci de `NULL AS ''`). Réglable sur la déclaration de source, surchargeable dans la requête. Raisons : la conversion se fait à la lecture, avant filtre et jointure, l'optimiseur sait que les colonnes sont nullables, et `CAST('' AS INTEGER)` échoue alors qu'avec un NULL il passe. Alternative ponctuelle : `NULLIF(col, '')`.
- La RFC 4180 ne dit rien sur le NULL : `,,` et `,"",` y sont deux écritures de la même chose. Les outils divergent (PostgreSQL : `,,` donne NULL et `,"",` donne chaîne vide ; MySQL : `\N` ; DuckDB : `nullstr` ; Excel et pandas : le vide est NULL ou NaN). Sources citées de mémoire, à vérifier. Par défaut, `,,` et `,"",` donnent tous deux une chaîne vide.
- Le mode « guillemets = chaîne vide, vide nu = NULL » (comme PostgreSQL) est **reporté**, mais **le lecteur doit garder en interne l'information « champ entre guillemets »**, sinon il ne pourra jamais la restituer.
- Pas d'inférence des booléens ni des guillemets comme signal de type : les guillemets ne portent pas de sens fiable. Conversion explicite par `CAST` ou `CASE`.

**Erreurs et exécution**

- Nombre de colonnes différent de l'en-tête : erreur avec numéro de ligne. Option `ON ERROR SKIP` pour ignorer et compter.
- Lecture en générateur, filtre et projection appliqués dès la lecture.
- Le lecteur CSV suit la RFC 4180 pour l'échappement (guillemet doublé), sans traiter le backslash comme caractère d'échappement par défaut. À couvrir par des tests.
- Statistiques : taille du fichier, lignes estimées par échantillonnage.
- Pour les gros fichiers, commande explicite d'**import dans le workspace** (SQLite avec index), pour ne lire qu'une fois un CSV joint plusieurs fois.

## 9. Workspace (matérialisation)

Usage : de l'analyse, avec plusieurs itérations sur un même jeu de données récupéré de plusieurs sources, pendant une ou deux journées de travail. Pas de cache à invalider, pas de réutilisation automatique de résultats (query matching).

- Un workspace est un **fichier SQLite par utilisateur ou par session**, avec quota disque et nettoyage.
- L'utilisateur enregistre explicitement un résultat sous un nom (`CREATE TABLE ws.ventes AS ...`), puis itère dessus avec des requêtes qui n'ont plus rien de fédéré : une seule source locale, indexée.
- Le workspace est une **source de plus** pour l'optimiseur, la meilleure : statistiques exactes, aucune latence réseau.
- Chaque jeu garde sa **provenance** (requête d'origine, sources, date), avec rafraîchissement possible.
- C'est un **cache jetable**, reconstructible depuis les sources : pas de transactions, de sauvegardes ni de migrations à gérer.
- Limites : SQLite est orienté lignes, donc les agrégations sur des dizaines de millions de lignes sont correctes avec de bons index mais pas rapides comme un moteur colonnaire. Insertion par lots dans une transaction.
- Le stockage est isolé derrière une petite interface (créer, interroger, lister, supprimer) : on pourra le remplacer par un moteur colonnaire plus tard.
- Sébastien n'exclut pas d'aller plus tard vers un vrai système de base de données.

## 10. Écritures, rôles et sécurité

- **Une source lue, une destination écrite.** Pas d'écriture vers plusieurs destinations dans une même requête (aucun cas d'usage). Exemples utiles : import (`INSERT INTO mysql.clients SELECT ... FROM csv.clients_2026`), export vers un CSV, copie ou migration d'une base vers une autre. Non atomique entre deux sources, l'utilisateur en est averti.
- **CSV** : en lecture, plus export d'un résultat vers un nouveau fichier. L'ajout en fin de fichier viendrait ensuite. `UPDATE` et `DELETE` sur CSV (réécriture complète du fichier, verrou) seulement si un besoin réel apparaît.
- **Rôles** : certains utilisateurs en lecture seule, d'autres avec droits d'administration sur les serveurs (suppression, mise à jour). Classification par l'AST, comptes de connexion distincts par rôle côté source, identifiants chiffrés, journal d'audit.
- **Multi-utilisateur** : workspace par utilisateur avec quota, requêtes longues dans des workers de longue durée, annulation possible.

## 11. Fonctions enrichies : catalogue

- Chaque fonction du catalogue a une définition par dialecte : native, émulée, ou **locale** (évaluée par Prisme après rapatriement, utile dans le moteur fédéré). Le catalogue indique à l'optimiseur ce qui est poussable, sinon il enverrait un filtre que la source ne peut pas exécuter.
- La sémantique de chaque fonction est définie une fois pour le dialecte de référence (début de semaine lundi ou dimanche, NULL, division entière, fuseaux) et vérifiée par des tests identiques sur chaque source.
- Commencer par un petit catalogue (dates, chaînes, conditionnels), puis ajouter à la demande. Le choix de la sémantique de PRQL et du SQL pipe (héritent-ils de la sémantique de référence ?) reste à trancher.

## 12. Exemples de requêtes

Les noms de sources (`mysql_a`, `csv`) sont des exemples.

**1. SQL enrichi : jointure MySQL et CSV**
```sql
SELECT c.nom, SUM(o.montant) AS total
FROM mysql_a.commandes o
JOIN csv('clients.csv') c ON c.id = o.client_id
WHERE o.date_cmd >= DATE '2026-01-01' AND c.pays = 'FR'
GROUP BY c.nom
```
Plan visé : le CSV est lu avec le filtre `pays = 'FR'` appliqué à la lecture ; ses `id` partent par lots vers MySQL (semi-join) ; MySQL exécute `SELECT client_id, SUM(montant) ... WHERE date_cmd >= ... AND client_id IN (...) GROUP BY client_id` (agrégation avant jointure) ; Prisme joint ce petit résultat au CSV puis refait un `SUM` par `nom`, valide pour un `SUM`.

**2. La même requête en Tutorial D flex**
```
SUMMARIZE (
  (mysql_a.commandes WHERE date_cmd >= DATE('2026-01-01'))
  JOIN
  ((csv.clients WHERE pays = 'FR') RENAME {id AS client_id})
) PER {nom} ADD (SUM(montant) AS total)
```
Le `JOIN` est naturel (sur les attributs de même nom), d'où le `RENAME`. Le plan logique est identique à celui de l'exemple 1.

**3. Syntaxe pipe avec fonction enrichie**
```sql
FROM mysql_a.commandes
|> WHERE date_trunc('week', date_cmd) = date_trunc('week', current_date)
|> AGGREGATE SUM(montant) AS total GROUP BY client_id
```
`current_date` est évalué une fois. Le filtre devient une plage de dates (`date_cmd >= '2026-09-28' AND date_cmd < '2026-10-05'` pour une exécution le 2 octobre 2026), ce qui permet à MySQL d'utiliser son index.

**4. Workspace et imbrication**
```sql
CREATE TABLE ws.cmd_fr AS
  SELECT o.client_id, o.produit FROM mysql_a.commandes o
  WHERE o.pays = 'FR';

SELECT client_id, array_agg(produit) AS produits
FROM ws.cmd_fr GROUP BY client_id;
```
La première instruction est fédérée et écrit dans SQLite. La seconde est une requête locale sur une seule source, avec un nœud `Group`.
