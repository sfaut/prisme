# Prisme

Prisme est un client web multi-bases de données, sous forme de notebook SQL : plusieurs sources, une seule vue. On y interroge MySQL, MariaDB, PostgreSQL, SQLite et DuckDB, dans leur dialecte natif ou dans un autre dialecte que Prisme traduit.

Le projet est en début de V1. Pour l'instant, Prisme prépare son dossier de données (clef maîtresse, base interne, utilisateur local) et démarre un serveur web minimal. Le périmètre et l'avancement sont décrits dans [docs/V1.md](docs/V1.md).

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

## Développer

```
cargo test                    # tests de tous les crates
cargo clippy --all-targets    # analyse statique, doit rester sans avertissement
cargo fmt                     # formatage
```

Organisation du dépôt :

```
crates/server/     exécutable prisme : configuration, clef maîtresse, base interne, API
  migrations/      migrations SQL de la base interne, numérotées
docs/              décisions et réflexions de conception
```

Les autres crates (`types`, `connectors`, `dialects`, etc.) et l'interface `web/` arrivent au fil de la V1. La cible est décrite dans [CLAUDE.md](CLAUDE.md).

## Documentation

- [docs/V1.md](docs/V1.md) : périmètre, décisions, stack Rust détaillée, ordre de développement.
- [docs/FEDERATED.md](docs/FEDERATED.md) : le moteur fédéré, après la V1.
- [docs/QUESTIONS.md](docs/QUESTIONS.md) : pistes retenues ou écartées, questions ouvertes.

## Licence

MIT, voir [LICENSE](LICENSE).
