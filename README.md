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

Prisme écoute alors sur <http://127.0.0.1:8484>. La route `/api/health` répond `ok`.

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

Puis ouvrir <http://localhost:5173>.

- **Explorateur** : sous la racine repliable « Sources de données », une branche par source (chaque connexion enregistrée, par son nom), ses bases en dessous, puis la racine « Notebooks », dont le menu « ⋮ » propose « Créer un notebook », inactif en attendant les notebooks ; sous chaque base, les dossiers Tables, Vues, Procédures et Fonctions ; sous chaque table, Colonnes, Index, Clefs étrangères et Déclencheurs. Le catalogue d'une source s'affiche au fil de l'eau dès qu'on l'ouvre, et celle de la connexion choisie s'ouvre d'elle-même (les bases, puis leurs objets, puis le détail des tables, en commençant par la base de la connexion), puis reste en mémoire. Un clic sur une ligne ouvre ou ferme son niveau. Un double clic sur une source la choisit comme connexion de travail ; un double clic sur une base en fait la base en cours, comme `USE` (au départ, c'est la base indiquée dans la connexion). Le menu « ⋮ » de la racine, affiché au survol de sa ligne comme celui de chaque source, propose « Ajouter une connexion » et « Tout rafraîchir » (relit les catalogues déjà lus) ; la fenêtre de création ou de modification a aussi un bouton « Tester », qui essaie la saisie avant de l'enregistrer ; celui de chaque source propose « Modifier » (mot de passe laissé vide : inchangé), « Rafraîchir », « Tester » (affiche la variante et la version du serveur) et « Supprimer », après confirmation. Un double clic sur une table ou une vue affiche son contenu ; sur une table d'une autre source, il choisit d'abord cette connexion, et la requête part dès que sa session est ouverte. La base en cours de la connexion choisie (celle de la connexion, puis celle choisie par `USE`) est en gras ; les colonnes d'une table s'affichent en trois colonnes alignées (symbole, nom, puis type, nullabilité et `AUTO_INCREMENT` éventuel en capitales, plus petits et plus clairs) ; les colonnes indexées (clef primaire comprise) sont en gras, la clef primaire est précédée d'une clef, les clefs étrangères sont précédées d'un croisillon ; la clef primaire porte la mention `PRIMARY KEY`, un autre index unique la mention `UNIQUE`. Un double clic sur une procédure, une fonction ou un déclencheur ne fait rien pour l'instant. Un libellé trop long est tronqué (…), et le type complet d'une colonne s'affiche au survol. Le panneau de gauche s'élargit en faisant glisser son bord droit ; sa largeur est retenue, et un double clic sur le bord la remet à 300 pixels.
- **Cellule SQL** : Ctrl-Entrée ou « Exécuter » lance la requête sous le curseur (entre deux requêtes, la précédente) ; Ctrl-Maj-Entrée ou « Tout exécuter » lance toutes les requêtes de l'éditeur en parallèle. Chaque requête a son onglet de résultat, avec une pastille d'avancement à gauche du libellé : grise et pulsant très lentement tant qu'elle attend son tour (au-delà de huit requêtes en parallèle), bleue et pulsante pendant l'exécution (lentement) puis pendant la réception des lignes (vite), verte une fois le résultat complet, rouge en cas d'échec. Au survol, la pastille donne le statut, la durée (qui court pendant l'exécution) et le nombre de lignes (« tronqué » si la limite en a écarté). « Stopper » arrête toutes les requêtes et tous les chargements en cours. « Enjoliver » réécrit le texte en « beau SQL » : blancs exotiques et tabulations remplacés par des espaces, `AS` devant les alias, noms de fonctions en capitales (sauf les fonctions et procédures de la base, dans la casse de leur définition), mots-clefs en capitales, une clause par ligne, une colonne et un prédicat par ligne quand il y en a plusieurs, espaces autour des opérateurs ; un texte avec `DELIMITER` est traité morceau par morceau, et le corps d'une procédure est mis en retrait bloc par bloc (Ctrl-Z annule ; règles dans la section 2.9 de `docs/V1.md`). Le bouton s'active une fois le catalogue de la connexion chargé, qui donne la casse des fonctions de la base. La liste à côté du bouton change la limite de lignes par défaut. Seules les lectures passent pour l'instant.
- **Grille** :

| Action | Geste |
|---|---|
| Trier | bouton ⇅ de l'en-tête : croissant, décroissant, aucun |
| Filtrer | champ « Filtrer », sur toutes les colonnes ou sur celle choisie à côté |
| Déplacer une colonne | glisser son nom (un repère vert montre où elle sera insérée) ; l'ordre du `SELECT` revient à la prochaine exécution |
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

Prisme répond `{"type":"ready",...}` une fois connecté au serveur cible. Ensuite, chaque ligne saisie est un message, en JSON : `run` pour exécuter, `cancel` pour arrêter toutes les requêtes en cours. `sql` contient une ou plusieurs requêtes ; avec `cursor` (position dans le texte), seule la requête sous le curseur s'exécute, sans `cursor` toutes s'exécutent en parallèle :

```
{"type": "run", "sql": "SELECT * FROM information_schema.COLUMNS", "cursor": 0}
{"type": "run", "sql": "SELECT * FROM information_schema.COLUMNS", "cursor": 0, "row_limit": 2000}
{"type": "run", "sql": "SELECT 1; SELECT 2"}
{"type": "cancel"}
```

Prisme annonce d'abord les requêtes du lot (`batch`, chacune numérotée), puis, pour chaque requête, des messages portant son numéro (`query`), dans l'ordre :

1. `started` : la requête part (en parallèle, elle peut attendre son tour) ;
2. `columns` : noms des colonnes, colonnes alignées à droite par la grille (nombres, dates et heures, d'après le type annoncé par le serveur), texte réellement exécuté, et traitement de la limite (`added` si Prisme a ajouté `LIMIT N + 1`, `user_limit` si la requête en avait déjà une, `not_applicable` pour `SHOW` ou `EXPLAIN`) ;
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
