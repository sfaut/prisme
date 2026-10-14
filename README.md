# Prisme

Prisme est un client web multi-bases de données, sous forme de notebook SQL : plusieurs sources, une seule vue. On y interroge MySQL, MariaDB, PostgreSQL, SQLite et DuckDB, dans leur dialecte natif ou dans un autre dialecte que Prisme traduit.

Le projet est en début de v1. Pour l'instant, Prisme prépare son dossier de données (clef maîtresse, base interne, utilisateur local) et expose une API JSON qui enregistre des connexions MySQL / MariaDB, les teste, et y exécute des requêtes en streaming par WebSocket, avec la limite de lignes par défaut. Une première interface web permet de choisir une connexion, d'explorer ses bases, tables et colonnes, et d'exécuter une cellule SQL. Le périmètre et l'avancement sont décrits dans [docs/V1.md](docs/V1.md).

## Prérequis

| Outil | Rôle | Installation sous Windows |
|---|---|---|
| Rust (via `rustup`) | Compilation du backend. La version exacte est figée par `rust-toolchain.toml` et installée automatiquement au premier `cargo`. | `winget install Rustlang.Rustup` |
| Visual Studio Build Tools (C++) | Linker MSVC, et compilation de SQLite embarqué. | `winget install Microsoft.VisualStudio.2022.BuildTools`, avec la charge de travail « Développement Desktop en C++ » |
| Docker Desktop | Bases MySQL, MariaDB et PostgreSQL des tests d'intégration (à venir). Le moteur doit être démarré. | `winget install Docker.DockerDesktop` |
| Node.js et npm | Interface Vue, en développement. | `winget install OpenJS.NodeJS` |

## Lancer Prisme

À la racine du dépôt :

```
cargo run
```

Prisme écoute alors sur <http://127.0.0.1:8484>, qui sert l'API (la route `/api/health` répond `ok`) et l'interface compilée, si elle l'a été (voir plus bas). Sinon, la page d'accueil dit comment la compiler.

Options (`cargo run -- --help`) :

| Option | Défaut | Rôle |
|---|---|---|
| `--listen` | `127.0.0.1:8484` | Adresse d'écoute. Elle doit être locale (`127.0.0.1` ou `::1`) : sans authentification, Prisme refuse d'être accessible depuis le réseau. |
| `--data-dir` | `%APPDATA%\Prisme` (Windows), `~/.local/share/Prisme` (Linux) | Dossier de données. |
| `--user-email` | `sebastien.faut@gmail.com` | Utilisateur unique, créé au premier lancement. |

Les options se passent après `--`, par exemple `cargo run -- --data-dir ./tmp` pour un dossier de données jetable (ignoré par Git).

Pour des journaux plus détaillés, définir `RUST_LOG` avant le lancement : `$env:RUST_LOG="debug"` sous PowerShell.

## Utiliser l'interface

En développement, l'interface tourne sur le serveur de Vite, qui redirige l'API vers Prisme. Il faut deux terminaux, à la racine du dépôt :

```
cargo run
```

```
cd web
npm install          # la première fois, ou après un changement de dépendances
npm run dev
```

Puis ouvrir <http://localhost:5173>. Vite recharge la page à chaque modification de l'interface.

Pour obtenir un seul exécutable, sans Node.js à l'exécution, compiler l'interface puis Prisme :

```
cd web
npm run build        # écrit l'interface compilée dans web/dist
cd ..
cargo build --release
```

L'exécutable `target/release/prisme.exe` contient alors `web/dist` : lancé seul, il sert l'interface sur <http://127.0.0.1:8484>. En développement, `cargo run` lit `web/dist` sur le disque : après un `npm run build`, il suffit de recharger la page, sans recompiler.

- **Explorateur** : sous la racine repliable « Sources de données », une branche par source (chaque connexion enregistrée, par son nom), ses bases en dessous, puis la racine « Notebooks », dont le menu « ⋮ » propose « Créer un notebook », inactif en attendant les notebooks ; sous chaque base, les dossiers Tables, Vues, Procédures et Fonctions ; sous chaque table, Colonnes, Index, Clefs étrangères et Déclencheurs. Une icône précède les sources (câble), les bases (base de données), les tables et les vues (tableau), et la racine « Notebooks » (carnet). Le catalogue d'une source s'affiche au fil de l'eau dès qu'on l'ouvre, et celle de la connexion choisie s'ouvre d'elle-même (les bases, puis leurs objets, puis le détail des tables, en commençant par la base de la connexion), puis reste en mémoire. Un clic sur une ligne ouvre ou ferme son niveau. Un double clic sur une source la choisit comme connexion de travail ; un double clic sur une base en fait la base en cours, comme `USE` (au départ, c'est la base indiquée dans la connexion). Le menu « ⋮ » de la racine, affiché au survol de sa ligne comme celui de chaque source, propose « Ajouter une connexion » et « Tout rafraîchir » (relit les catalogues déjà lus) ; la fenêtre de création ou de modification a aussi un bouton « Tester », qui essaie la saisie avant de l'enregistrer, et un champ « À la connexion » pour des requêtes exécutées à l'ouverture de chaque connexion vers la base, séparées par des « ; » (`SET NAMES utf8mb4; SET SESSION sql_mode = ...`) : seuls les réglages de la session sont permis, un réglage global (`SET GLOBAL`) ou toute autre requête est refusé à l'enregistrement ; celui de chaque source propose « Modifier » (mot de passe laissé vide : inchangé), « Rafraîchir », « Tester » (affiche la variante et la version du serveur) et « Supprimer », après confirmation. Un double clic sur une table ou une vue affiche son contenu ; sur une table d'une autre source, il choisit d'abord cette connexion, et la requête part dès que sa session est ouverte. La base en cours de la connexion choisie (celle de la connexion, puis celle choisie par `USE`) est en gras ; les colonnes d'une table s'affichent en trois colonnes alignées (symbole, nom, puis type, nullabilité et `AUTO_INCREMENT` éventuel en capitales, plus petits et plus clairs) ; les colonnes indexées (clef primaire comprise) sont en gras, la clef primaire est précédée d'une clef, les clefs étrangères sont précédées d'un croisillon ; la clef primaire porte la mention `PRIMARY KEY`, un autre index unique la mention `UNIQUE`. Un double clic sur une procédure, une fonction ou un déclencheur ne fait rien pour l'instant. Un libellé trop long est tronqué (…), et le type complet d'une colonne s'affiche au survol. Le panneau de gauche s'élargit en faisant glisser son bord droit, marqué d'une poignée en son milieu, comme la barre de partage de la cellule SQL ; sa largeur est retenue, et un double clic sur le bord la remet à 300 pixels. En haut du panneau, la liste « Système / Clair / Sombre » choisit le thème de toute l'interface, éditeur SQL compris ; « Système » suit le réglage du poste.
- **Éditeurs** : au-dessus de la cellule SQL, une barre d'onglets ouvre plusieurs éditeurs à la fois, chacun avec sa connexion, sa propre session (donc ses transactions) et ses onglets de résultat. « + » ouvre un éditeur sur la connexion de l'éditeur actif ; un double clic sur un onglet le renomme (par défaut, il porte le nom de sa connexion) ; la croix (ou un clic du milieu) le ferme, et il reste rouvrable 24 heures depuis le bouton « Rouvrir » (horloge), puis il est supprimé ; un onglet se déplace en le faisant glisser. L'explorateur agit sur l'éditeur actif : un double clic sur une source en fait sa connexion, une table s'y affiche, une base y devient la base en cours. Tout est enregistré dans la base interne : textes (au fil de la frappe), éditeurs, onglets et résultats, lignes comprises (le serveur les enregistre lui-même pendant l'exécution) ; après un rafraîchissement de la page ou la fermeture du navigateur, tout revient tel quel. Une requête interrompue par la fermeture de la page est enregistrée en échec.
- **Cellule SQL** : Ctrl-Entrée ou « Exécuter » lance la requête sous le curseur (entre deux requêtes, la précédente) ; Ctrl-Maj-Entrée ou « Tout exécuter » lance toutes les requêtes de l'éditeur en parallèle. Chaque requête a son onglet de résultat, avec une pastille d'avancement à gauche du libellé : grise et pulsant très lentement tant qu'elle attend son tour (au-delà de huit requêtes en parallèle), bleue et pulsante pendant l'exécution (lentement) puis pendant la réception des lignes (vite), verte une fois le résultat complet, rouge en cas d'échec. Chaque onglet porte sur son bord bas une barre d'avancement : un segment bleu qui va et vient tant que la part accomplie est inconnue (exécution), une barre qui se remplit pendant la réception quand Prisme a fixé la limite de lignes, verte une fois la requête terminée, rouge en cas d'échec. Un double clic sur le libellé d'un onglet le renomme (Entrée valide, Échap annule, un nom vide rend le libellé par défaut). L'épingle d'un onglet le garde quand les requêtes sont relancées : les onglets épinglés restent à gauche, les nouveaux résultats arrivent à leur suite. La croix (ou un clic du milieu) ferme l'onglet et arrête sa requête si elle tourne encore ; les autres requêtes du lot vont au bout. Épingle et croix apparaissent au survol de l'onglet et sur l'onglet actif. Un onglet se déplace en le faisant glisser, comme une colonne de la grille : un repère bleu montre où il sera inséré. Sous la grille, une barre donne le nombre de lignes chargées et deux durées : l'exécution de la requête, mesurée par le serveur jusqu'à l'envoi des colonnes, puis le transfert et l'affichage des lignes, suivi du volume reçu (en o, ko, Mo, Go selon l'ordre de grandeur) et du débit (« exécution 120 ms · transfert 2,4 s · 3,1 Mo · 1,3 Mo/s ») ; ils sont rafraîchis une fois par seconde pendant la requête. Une durée s'affiche en microsecondes sous la milliseconde, en millisecondes sous la seconde, en secondes sous la minute, en minutes et secondes sous l'heure, puis en heures et minutes. « Stopper » arrête toutes les requêtes et tous les chargements en cours. « Enjoliver » réécrit le texte en « beau SQL » : blancs exotiques et tabulations remplacés par des espaces, `AS` devant les alias, noms de fonctions en capitales (sauf les fonctions et procédures de la base, dans la casse de leur définition), mots-clefs en capitales et noms de bases, tables et colonnes dans l'écriture du catalogue, d'après l'analyse de la requête (un mot employé comme mot-clef, comme `CAST`, passe en capitales ; une colonne qui s'appelle `cast` reste un nom ; un nom inconnu ou ambigu reste tel quel ; un mot-clef inconnu de la version du serveur aussi), une clause par ligne, une colonne et un prédicat par ligne quand il y en a plusieurs, espaces autour des opérateurs ; un texte avec `DELIMITER` est traité morceau par morceau, et le corps d'une procédure est mis en retrait bloc par bloc (Ctrl-Z annule ; règles dans la section 2.9 de `docs/V1.md`). Le bouton s'active une fois le catalogue de la connexion chargé, qui donne l'écriture des tables, des colonnes et des fonctions de la base. La liste à côté du bouton change la limite de lignes par défaut. Les boutons sont au-dessus de l'éditeur. Une barre horizontale, tracée d'un trait sous l'éditeur, partage la hauteur entre l'éditeur (avec ses boutons) et les résultats : le tiers du haut pour l'éditeur par défaut ; la faire glisser change ce partage, qui est retenu d'une visite à l'autre, et un double clic la remet au tiers du haut. Seules les lectures passent pour l'instant. Quand l'éditeur contient plusieurs requêtes, celle que Ctrl-Entrée exécuterait est surlignée. Une requête que Prisme ne comprend pas est soulignée en rouge dès la frappe, avec le message au survol, avant toute exécution. La coloration et l'enjoliveur partagent les mots-clefs de chaque dialecte, un fichier par dialecte à modifier directement (`crates/dialects/keywords/mysql.yaml`, `mariadb.yaml`) : mots réservés, mots-clefs non réservés, types et valeurs, avec, au besoin, la version d'apparition d'un mot (`since`) ; l'éditeur ne colore que les mots de la version du serveur. La complétion (Ctrl-Espace, ou en tapant) propose seulement des noms, d'après le catalogue chargé par l'explorateur : les bases, les tables, vues, fonctions et procédures de la base en cours sans préfixe, et les colonnes après `table.` ou `alias.`, avec leur type. Les mots-clefs (SELECT, UPDATE...) ne sont pas proposés, et une proposition commence par le texte tapé (ou en reprend les initiales).
- **Grille** :

| Action | Geste |
|---|---|
| Trier | bouton ⇅ de l'en-tête : croissant, décroissant, aucun |
| Filtrer | bouton du filtre, à gauche de la barre sous la grille : il déplie le champ « Filtrer », sur toutes les colonnes ou sur celle choisie à côté, et le replie (le filtre reste appliqué, le bouton passe en bleu). La barre donne alors les lignes retenues sur les lignes chargées |
| Déplacer une colonne | glisser son nom (un repère bleu montre où elle sera insérée) ; l'ordre du `SELECT` revient à la prochaine exécution |
| Élargir / rétrécir une colonne | glisser le bord droit de son en-tête ; double clic sur ce bord pour l'ajuster à sa valeur la plus longue (720 pixels au plus), un second double clic pour revenir à la largeur estimée. Les largeurs choisies reviennent quand la même requête est relancée. Un texte trop long est tronqué (…), entier au survol |
| Sélectionner | clic sur une cellule, un numéro de ligne ou un nom de colonne ; glisser pour une plage, ou sur les numéros de ligne pour un groupe de lignes ; clic sur le coin supérieur gauche pour tout sélectionner |
| Étendre / ajouter | Maj-clic / Ctrl-clic |
| Tout sélectionner, vider | Ctrl-A, Échap |
| Copier | Ctrl-C, avec les noms des colonnes en première ligne (texte tabulé, collable dans un tableur ; NULL copié vide) |

## Données locales

Le dossier de données contient deux fichiers :

- **`prisme.db`** : la base interne SQLite (utilisateurs, puis connexions et notebooks). Son schéma est mis à jour automatiquement au démarrage.
- **`prisme.key`** : la clef maîtresse, 32 octets aléatoires encodés en base64, générée au premier lancement. Elle chiffre les mots de passe des connexions enregistrées.

Ces deux fichiers vont ensemble :

- **Sauvegarder** : copier les deux. Sans la clef, les mots de passe enregistrés sont illisibles et doivent être ressaisis. Le reste des données est intact.
- **Protéger** : quiconque possède les deux fichiers lit les mots de passe enregistrés. Le dossier de données doit rester lisible par le seul utilisateur, ce qui est le cas par défaut de `%APPDATA%`.
- **Ne jamais modifier `prisme.key` à la main** : Prisme refuse de démarrer avec une clef invalide, et ne l'écrase jamais.

## Tester en développement

### Tests automatisés

```
cargo test                           # tous les tests rapides, sans Docker
cargo test -p prisme-server          # les tests d'un seul crate
cargo test connections               # les tests dont le nom contient « connections »
cargo test -- --include-ignored      # en plus, les tests sur de vrais serveurs MySQL et MariaDB
cargo clippy --all-targets           # analyse statique, doit rester sans avertissement
cargo fmt                            # formatage
```

Côté interface, dans `web/` :

```
npm run test:unit -- --run           # tests Vitest (sans --run : relancés à chaque modification)
npm run type-check                   # vérification des types, templates compris (vue-tsc)
npm run lint                         # Oxlint et ESLint
npm run format                       # formatage (Prettier)
```

Les types TypeScript de `web/src/api/generated/` sont régénérés depuis Rust par `cargo test` : après un changement d'une structure échangée côté Rust, relancer `cargo test` puis `npm run type-check`.

Deux sortes de tests côté Rust :

- **Tests unitaires**, en bas de chaque fichier `.rs`, dans un bloc `#[cfg(test)] mod tests`. Ils couvrent aussi l'API : les requêtes HTTP sont envoyées à l'application en mémoire, sur une base interne en mémoire, sans ouvrir de port.
- **Tests d'intégration**, dans le dossier `tests/` de chaque crate. Ceux du connecteur MySQL / MariaDB démarrent de vrais serveurs dans Docker. Ils sont ignorés par défaut : il faut démarrer Docker Desktop, puis ajouter `-- --include-ignored`. Le premier lancement télécharge les images `mysql:8.1` et `mariadb:11.3`.

### Tester l'application à la main

Il n'y a pas encore d'interface : on teste l'API directement. Lancer Prisme sur un dossier de données jetable, à supprimer pour repartir de zéro :

```
cargo run -- --data-dir ./tmp
```

Puis, dans un second terminal PowerShell :

```powershell
$api = "http://127.0.0.1:8484/api"

# Le serveur répond-il ?
Invoke-RestMethod "$api/health"

# Enregistrer une connexion
$connection = @{
    name = "local"; host = "127.0.0.1"; port = 3306
    username = "root"; password = "mot de passe"; database_name = ""
} | ConvertTo-Json
Invoke-RestMethod "$api/connections" -Method Post -ContentType "application/json" -Body $connection

# Lister les connexions (le mot de passe n'apparaît jamais)
Invoke-RestMethod "$api/connections"

# Se connecter : renvoie la variante (mysql ou mariadb) et la version du serveur
Invoke-RestMethod "$api/connections/1/probe" -Method Post

# Limite de lignes par défaut : lire, puis changer (1000, 2000, 5000, 10000 ou 50000)
Invoke-RestMethod "$api/settings"
Invoke-RestMethod "$api/settings" -Method Put -ContentType "application/json" -Body '{"row_limit": 5000}'
```

En cas d'erreur (statut 4xx ou 5xx), PowerShell lève une exception, et le message de Prisme figure dans le corps JSON `{"error": "..."}`.

| Route | Rôle | Erreurs |
|---|---|---|
| `GET /api/health` | Répond `ok`. | — |
| `GET /api/connections` | Liste les connexions, sans mot de passe. | — |
| `POST /api/connections` | Enregistre une connexion, renvoie `{"id": ...}` (201). | 400 saisie invalide, 409 nom déjà pris |
| `POST /api/connections/{id}/probe` | Se connecte, renvoie `{"flavor": ..., "version": ...}`. | 404 connexion inconnue, 502 serveur injoignable ou accès refusé, 504 pas de réponse en 10 s |
| `GET /api/settings` | Réglages : `{"row_limit": 1000, "row_limit_choices": [...]}`. | — |
| `PUT /api/settings` | Change la limite par défaut, renvoie les réglages à jour. | 400 valeur hors des choix |

### Exécuter des requêtes

Les requêtes passent par un WebSocket, une session par connexion enregistrée : `ws://127.0.0.1:8484/api/connections/{id}/query`. Le plus simple pour l'essayer est `wscat`, via Node.js (le premier lancement propose de l'installer) :

```
npx wscat -c ws://127.0.0.1:8484/api/connections/1/query
```

Prisme répond `{"type":"ready",...}` une fois connecté au serveur cible. Ensuite, chaque ligne saisie est un message, en JSON : `run` pour exécuter, `cancel` pour arrêter toutes les requêtes en cours, `cancel_query` pour n'en arrêter qu'une, désignée par son numéro dans le lot. `sql` contient une ou plusieurs requêtes ; avec `cursor` (position dans le texte), seule la requête sous le curseur s'exécute, sans `cursor` toutes s'exécutent en parallèle :

```
{"type": "run", "sql": "SELECT * FROM information_schema.COLUMNS", "cursor": 0}
{"type": "run", "sql": "SELECT * FROM information_schema.COLUMNS", "cursor": 0, "row_limit": 2000}
{"type": "run", "sql": "SELECT 1; SELECT 2"}
{"type": "cancel"}
{"type": "cancel_query", "query": 1}
```

Prisme annonce d'abord les requêtes du lot (`batch`, chacune numérotée), puis, pour chaque requête, des messages portant son numéro (`query`), dans l'ordre :

1. `started` : la requête part (en parallèle, elle peut attendre son tour) ;
2. `columns` : noms des colonnes, colonnes alignées à droite par la grille (nombres, dates et heures, d'après le type annoncé par le serveur), texte réellement exécuté, et traitement de la limite (`added` si Prisme a ajouté `LIMIT N + 1`, `user_limit` si la requête en avait déjà une, `not_applicable` pour `SHOW` ou `EXPLAIN`), nombre de lignes au plus (`expected_rows` : la limite de Prisme ou celle écrite par l'utilisateur, `null` si inconnu) et temps d'exécution mesuré par le serveur (`execution_us`, en microsecondes) ;
3. `rows` : les lignes, par lots de 500, chaque cellule en texte ou `null` ;
4. `done` : nombre de lignes, `truncated` (des lignes au-delà de la limite ont été écartées), lignes modifiées, durée.

En cas d'échec ou d'arrêt, un message `error` remplace la suite de cette requête, et la session continue. `batch_done` signale la fin du lot. En parallèle, les messages des différentes requêtes s'entremêlent.

Seules les lectures passent : `SELECT`, `SHOW`, `DESCRIBE`, `EXPLAIN` et `USE`. Les écritures (`UPDATE`, `DELETE`, `DROP`, `CALL`...) sont refusées en attendant les interrupteurs d'écriture, ainsi que les requêtes que Prisme ne sait pas analyser.

Pour disposer d'un serveur jetable sans toucher à ses bases habituelles (Docker Desktop démarré) :

```
docker run --rm -d --name prisme-mariadb -p 3307:3306 -e MARIADB_ROOT_PASSWORD=secret mariadb:11
docker run --rm -d --name prisme-mysql   -p 3308:3306 -e MYSQL_ROOT_PASSWORD=secret   mysql:8
```

Puis enregistrer une connexion vers `127.0.0.1`, port `3307` ou `3308`, compte `root`, mot de passe `secret`. `docker stop prisme-mariadb prisme-mysql` les supprime.

Pour regarder la base interne, ouvrir `./tmp/prisme.db` avec n'importe quel client SQLite (DBeaver, DB Browser for SQLite, `sqlite3`). La colonne `password_secret` ne contient que des octets chiffrés.

## Organisation du dépôt

```
crates/
  connectors/      accès aux bases cibles (MySQL / MariaDB pour l'instant)
    tests/         tests d'intégration sur de vrais serveurs (Docker)
  dialects/        analyse des requêtes : limite par défaut, lecture / écriture
  server/          exécutable prisme : configuration, clef maîtresse, base interne, API
    migrations/    migrations SQL de la base interne, numérotées
web/               interface Vue
  src/api/         appels de l'API et session de requêtes
    generated/     types TypeScript générés depuis Rust (ne pas modifier)
  src/components/  composants de l'interface
  src/stores/      état partagé (connexions, réglages)
docs/              décisions et réflexions de conception
```

Les autres crates (`types`, `splitter`, etc.) arrivent au fil de la v1. La cible est décrite dans [CLAUDE.md](CLAUDE.md).

## Documentation

- [docs/V1.md](docs/V1.md) : périmètre, décisions, stack Rust détaillée, ordre de développement.
- [docs/FEDERATED.md](docs/FEDERATED.md) : le moteur fédéré, après la v1.
- [docs/QUESTIONS.md](docs/QUESTIONS.md) : pistes retenues ou écartées, questions ouvertes.

## Licence

MIT, voir [LICENSE](LICENSE).
