# Prisme

Prisme est un client web multi-bases de données, sous forme de notebook SQL : plusieurs sources, une seule vue. On y interroge MySQL, MariaDB, PostgreSQL, SQLite et DuckDB, dans leur dialecte natif ou dans un autre dialecte que Prisme traduit.

Le projet est en début de V1. Pour l'instant, Prisme prépare son dossier de données (clef maîtresse, base interne, utilisateur local) et expose une API JSON qui enregistre des connexions MySQL / MariaDB, les teste, et y exécute des requêtes en streaming par WebSocket, avec la limite de lignes par défaut. L'interface web n'existe pas encore. Le périmètre et l'avancement sont décrits dans [docs/V1.md](docs/V1.md).

## Prérequis

| Outil | Rôle | Installation sous Windows |
|---|---|---|
| Rust (via `rustup`) | Compilation du backend. La version exacte est figée par `rust-toolchain.toml` et installée automatiquement au premier `cargo`. | `winget install Rustlang.Rustup` |
| Visual Studio Build Tools (C++) | Linker MSVC, et compilation de SQLite embarqué. | `winget install Microsoft.VisualStudio.2022.BuildTools`, avec la charge de travail « Développement Desktop en C++ » |
| Docker Desktop | Bases MySQL, MariaDB et PostgreSQL des tests d'intégration (à venir). Le moteur doit être démarré. | `winget install Docker.DockerDesktop` |
| Node.js et npm | Interface Vue (à venir). | `winget install OpenJS.NodeJS` |

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

Deux sortes de tests :

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

Prisme répond `{"type":"ready",...}` une fois connecté au serveur cible. Ensuite, chaque ligne saisie est une requête, en JSON :

```
{"sql": "SELECT * FROM information_schema.COLUMNS"}
{"sql": "SELECT * FROM information_schema.COLUMNS", "row_limit": 2000}
{"sql": "SELECT * FROM information_schema.COLUMNS LIMIT 5"}
```

Prisme renvoie, dans l'ordre :

1. `columns` : noms des colonnes, texte réellement exécuté, et traitement de la limite (`added` si Prisme a ajouté `LIMIT N + 1`, `user_limit` si la requête en avait déjà une, `not_applicable` pour `SHOW`, `EXPLAIN` ou une écriture, `unparsed` si la requête n'a pas pu être analysée) ;
2. `rows` : les lignes, par lots de 500, chaque cellule en texte ou `null` ;
3. `done` : nombre de lignes, `truncated` (des lignes au-delà de la limite ont été écartées), lignes modifiées, durée.

En cas d'échec, un message `error` remplace la suite, et la session continue. Une seule requête par message : `SELECT 1; SELECT 2` est refusé. Attention : les écritures (`UPDATE`, `DELETE`, `DROP`...) sont exécutées, les interrupteurs d'écriture n'existant pas encore.

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
  server/          exécutable prisme : configuration, clef maîtresse, base interne, API
    migrations/    migrations SQL de la base interne, numérotées
docs/              décisions et réflexions de conception
```

Les autres crates (`types`, `dialects`, `splitter`, etc.) et l'interface `web/` arrivent au fil de la V1. La cible est décrite dans [CLAUDE.md](CLAUDE.md).

## Documentation

- [docs/V1.md](docs/V1.md) : périmètre, décisions, stack Rust détaillée, ordre de développement.
- [docs/FEDERATED.md](docs/FEDERATED.md) : le moteur fédéré, après la V1.
- [docs/QUESTIONS.md](docs/QUESTIONS.md) : pistes retenues ou écartées, questions ouvertes.

## Licence

MIT, voir [LICENSE](LICENSE).
