# Copie et export des résultats

Ce document regroupe les idées retenues pour copier la sélection de la grille dans plusieurs formats et pour exporter un résultat vers un fichier. Rien n'est implémenté. Les points tranchés par Sébastien sont marqués **[décidé]**, les autres restent ouverts (section 7).

## 1. Point de départ

- Ctrl-C copie la sélection de la grille en texte tabulaire (tabulations, guillemets à la façon des tableurs), avec les noms des colonnes en première ligne (`selectionToText()` dans `web/src/components/resultGrid.ts`).
- Les cellules arrivent en texte (`string | null`), avec le type de chaque colonne (`family`, nom natif).
- Le serveur ne garde pas les lignes : l'interface n'a que les N premières (limite par défaut), le reste n'a jamais été lu.

## 2. Formats

Chaque format est un sérialiseur pur (colonnes, types, lignes, options → texte), testé à part. `selectionToText()` se limite à extraire la sélection (lignes et colonnes couvertes, dans l'ordre affiché) et la confie au sérialiseur choisi.

| Format | NULL | Règles |
|---|---|---|
| Tabulaire | vide | Comportement actuel, pour coller dans un tableur **[décidé]**. Une version `text/html` est déposée en même temps dans le presse-papiers (`ClipboardItem`), pour qu'Excel ou un courriel reçoive un vrai tableau. |
| Box-Unicode | à fixer | Tableau dessiné avec les caractères de cadre (`┌─┬─┐`, `│`, `├─┼─┤`), dans l'esprit de la sortie du client `mysql` **[décidé]**. Nombres, dates et heures alignés à droite, comme dans la grille. Variante ASCII (`+---+`) possible. |
| CSV | vide, réglable (`\N`, `NULL`) | RFC 4180. Séparateur `,` ou `;` (Excel en français). |
| TSV | `\N` | TSV strict : `\t`, `\n` et `\\` échappés, comme `LOAD DATA` et `COPY`, pour recharger en base sans perte. |
| JSON | `null` | Tableau d'objets. |
| NDJSON | `null` | Un objet par ligne. |
| HTML | cellule vide, classe `null` | `<table>` avec `<thead>`, entités échappées. |
| XML | `xsi:nil="true"` | Forme de `mysql --xml` : `<row><field name="…">`, qui accepte tout nom de colonne. |

Règles communes :

- **Nombres en JSON** **[décidé]** : les colonnes entières, flottantes et décimales s'écrivent en nombres JSON, recopiés tels que le serveur les a envoyés (jamais convertis par `Number`, donc sans perte sur BIGINT et DECIMAL à l'écriture). À ajuster si la relecture par JavaScript pose problème.
- **Colonnes JSON** **[décidé]** : une valeur reconnue comme JSON (`contentKind.ts`) s'insère comme objet imbriqué en JSON et NDJSON.
- **Noms de colonnes en double** (`SELECT a.id, b.id`) **[décidé]** : l'export est refusé, quel que soit le format, avec une erreur explicite qui nomme les colonnes en cause. Le risque écarté est l'écrasement silencieux d'une colonne par une autre.
- **Sélection discontinue** (Ctrl-clic) : les formats en grille (tabulaire, Box-Unicode, CSV, TSV, HTML, images) laissent la cellule non sélectionnée vide ; les formats à objets (JSON, NDJSON, XML) omettent sa clef. Une cellule non sélectionnée ne se confond ainsi jamais avec un NULL.

### 2.1 Images : PNG, WebP, SVG

Le tableau s'exporte aussi en image **[décidé]** : PNG et WebP, plus SVG en variante vectorielle.

- **Rendu dans le navigateur** : le tableau est dessiné directement sur un `canvas` (`fillText`, traits), puis `canvas.toBlob('image/png')` ou `toBlob('image/webp')` produit le fichier. Les deux encodages sont natifs, sans dépendance.
- **Mise en page commune** : un module pur calcule la position de chaque cellule (largeurs, alignement à droite des nombres, dates et heures, ordre des colonnes affiché). Le canvas et le SVG reprennent ce même calcul ; le SVG garde un texte sélectionnable et reste net à toute échelle.
- **Thème** : couleurs, police et espacements viennent des variables CSS du thème (`web/src/assets/main.css`).
- **Netteté** : dimensions multipliées par `devicePixelRatio` ; dessin après `document.fonts.ready`, pour que la police du thème soit chargée.
- **Capture du DOM écartée** (`html-to-image`) : la grille est virtuelle, seules les lignes visibles existent dans la page ; il faudrait la reconstruire hors écran.
- **Portée** : la sélection ou les lignes chargées. Les navigateurs plafonnent le canvas (environ 16 000 pixels de côté, 268 millions de pixels de surface selon le navigateur), soit quelques centaines de lignes : au-delà, une erreur explicite indique le plafond. L'export complet par le serveur ne s'applique pas aux images.
- **Presse-papiers** : il n'accepte que `image/png` (`ClipboardItem`). « Copier en image » produit donc du PNG ; WebP et SVG passent par l'export en fichier.

## 3. Interface de la copie

Prisme n'utilise pas le clic droit jusqu'ici, et il vaut mieux continuer ainsi. Pistes :

- **Ctrl-C** copie dans le format par défaut (tabulaire au départ). Ce format par défaut rejoint le réglage du profil déjà prévu pour les en-têtes (`docs/QUESTIONS.md`, point 27).
- **Bouton « Copier »** dans la barre du bas de la grille, avec un menu déroulant (`DropdownMenu` de Reka UI) : un format par entrée, plus « sans en-têtes ».
- **Ctrl-Maj-C** recopie dans le dernier format choisi depuis ce menu.

## 4. Export vers un fichier

Deux portées :

- **Lignes chargées** : tout se fait dans le navigateur, avec les sérialiseurs de la copie, un `Blob` et un téléchargement. Immédiat.
- **Résultat complet** : le serveur réexécute la requête sans le `LIMIT` ajouté par Prisme ; une limite écrite par l'utilisateur passe telle quelle (règle 3 de `CLAUDE.md`). Seules les lectures sont acceptées, avec la classification par l'AST déjà en place.

Fenêtre d'export, ouverte par un bouton de la barre du bas :

- portée (lignes chargées, ou résultat complet avec l'avertissement « la requête sera réexécutée ») ;
- format (ceux de la section 2, images comprises pour les lignes chargées) ;
- options : en-têtes, NULL, séparateur, BOM UTF-8 pour Excel ;
- nom de fichier proposé : `<onglet>-AAAAMMJJ-HHMMSS.<extension>`.

### 4.1 État de la grille **[décidé]**

L'export suit l'état de la grille : ordre des colonnes, tri et filtre. Pour les lignes chargées, c'est direct (la vue existe déjà). Pour le résultat complet, le serveur doit reproduire la même vue sur toutes les lignes (voir section 7, question 2).

### 4.2 Gestionnaire d'exports

L'export complet tourne en tâche de fond, et un gestionnaire d'exports prévient quand le fichier est prêt à télécharger. Idées :

- le serveur lit le flux du pilote et écrit directement dans un fichier temporaire du dossier de données, avec une mémoire constante ;
- le gestionnaire liste les exports (en cours, prêts, en échec, annulés), avec le nombre de lignes écrites et la taille ;
- une notification signale la fin, puis le fichier se télécharge par `GET /api/exports/{id}` (`Content-Disposition: attachment`) ;
- annuler un export arrête la requête (`KILL QUERY`, comme `cancel`) et supprime le fichier partiel ;
- les fichiers prêts sont supprimés après téléchargement ou après un délai ;
- ce mécanisme annonce le nœud `Export` du moteur fédéré (`docs/FEDERATED.md`, section 8).

### 4.3 Sérialiseurs en double

L'export complet demande des sérialiseurs en Rust, alors que ceux de la copie sont en TypeScript. Pour que les deux restent identiques, des fichiers de référence communs (`tests/fixtures/export/` : une entrée en JSON, la sortie attendue par format) sont lus par les deux suites de tests, dans l'esprit de la suite de conformité.

## 5. Ordre de réalisation envisagé

1. Sérialiseurs TypeScript et leurs tests.
2. Bouton « Copier », Ctrl-Maj-C et dépôt `text/html` du format tabulaire.
3. Export des lignes chargées.
4. Images : mise en page commune, PNG et WebP par le canvas, SVG.
5. Sérialiseurs Rust et fichiers de référence communs.
6. Gestionnaire d'exports et export complet.

## 6. Tableau des décisions

| Sujet | Décision |
|---|---|
| Tabulaire et TSV | Deux formats : tabulaire pour les tableurs, TSV strict pour recharger en base. |
| Box-Unicode | Ajouté à la liste. |
| Images | PNG et WebP par le canvas du navigateur, SVG en variante vectorielle ; PNG seul dans le presse-papiers. |
| Nombres en JSON | Nombres JSON, à ajuster si besoin. |
| Colonnes JSON | Objets imbriqués. |
| Noms de colonnes en double | Export refusé, quel que soit le format. |
| État de la grille | L'export suit l'ordre des colonnes, le tri et le filtre. |
| Clic droit | À éviter. |

## 7. Questions ouvertes

1. **Connexion de l'export complet** : une connexion dédiée laisse la session libre, mais ne voit ni les tables temporaires, ni les variables de session, ni une future transaction en cours. L'exécution dans la session voit tout cela, mais l'occupe pendant l'export. Le gestionnaire d'exports pousse vers la connexion dédiée ; à réfléchir.
2. **Tri et filtre du résultat complet** : le tri peut passer dans le SQL (`SELECT * FROM (…) AS t ORDER BY …`), au risque d'un ordre différent de celui de la grille (collation du serveur et comparaison de `compareCells()`), ce que la règle 2 de `CLAUDE.md` interdit de faire en silence. Le filtre peut s'appliquer côté serveur sur le flux, avec une réimplémentation en Rust de celui de la grille. Trier côté serveur hors base oblige à tout garder en mémoire ou sur disque.
3. **NULL en Box-Unicode** : `NULL` affiché tel quel se confond avec la chaîne `'NULL'` ; un symbole (`∅`) ou un style distinct est à choisir.
4. **Largeur en Box-Unicode** : l'alignement demande la largeur d'affichage des caractères (caractères larges d'Asie orientale, emojis), différente de leur nombre.
5. **Noms en double et copie** : le refus vaut-il aussi pour la copie dans le presse-papiers, y compris en tabulaire, où l'écrasement n'existe pas ?
6. **Colonne JSON invalide** : une valeur d'une colonne JSON qui ne s'analyse pas reste-t-elle une chaîne (le type change alors d'une ligne à l'autre), ou l'export échoue-t-il ?
7. **Volume du presse-papiers** : faut-il plafonner la copie (nombre de cellules), et proposer l'export au-delà ?
8. **Thème des images** : celui de l'écran, ou toujours le thème clair, plus lisible dans un document imprimé ou un courriel ? Un choix dans la fenêtre d'export est possible.
