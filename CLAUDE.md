# Prisme

Client web multi-bases de données : plusieurs sources, une seule vue. Prisme se présente comme un carnet de notes (notebook) SQL où l'on interroge MySQL, MariaDB, PostgreSQL, SQLite et DuckDB, dans leur dialecte natif ou dans un autre dialecte que Prisme traduit. À terme, un moteur fédéré croise plusieurs sources (bases, CSV) dans une même requête.

Ce fichier est court. Les décisions détaillées sont dans `docs/` :

- `docs/V1.md` : périmètre, décisions techniques et ordre de développement de la v1 (à lire en premier).
- `docs/FEDERATED.md` : vision après la v1 (plan logique, optimiseur, langages, NULL, CSV, workspace).
- `docs/QUESTIONS.md` : pistes retenues, écartées ou reportées, et questions encore ouvertes.
- `docs/PERF.md` : problèmes de performance rencontrés, avec leur cause, leur correction et la méthode de mesure.

## Qui travaille ici

Sébastien est propriétaire du projet, travaille en backend et en données (PHP, Vue, MySQL/MariaDB), et ne pratique pas Rust. Claude Code écrit l'essentiel du code, mais Sébastien doit pouvoir le relire, le corriger et le faire évoluer sans aide. Conséquences :

- Modules petits et nommés, fonctions courtes, pas d'abstraction inutile.
- Un commentaire de module en tête de chaque fichier important, qui dit ce que fait le module et pourquoi il existe.
- Des tests par module, plus la suite de conformité (voir plus bas).
- Les échanges et les documents sont en français. Les identifiants du code sont en anglais.
- Dans la prose (documents, commentaires, messages), une version s'écrit en minuscule : « v1 », « v2 ». Les noms de fichiers gardent leur forme (`docs/V1.md`).

## Stack (décidée)

- **Backend** : Rust, `tokio` + `axum`, WebSocket pour le streaming des résultats et l'annulation.
- **Pilotes natifs, un par moteur** : `mysql_async` (MySQL et MariaDB), `tokio-postgres`, `rusqlite`, `duckdb` (derrière une feature Cargo, car la compilation est longue). `rusqlite` et `duckdb` sont synchrones : les mettre dans des tâches bloquantes dédiées.
- **Parseur SQL** : `sqlparser-rs`. À vérifier : pas de dialecte MariaDB à ma connaissance, donc extension du dialecte MySQL.
- **Base interne** : SQLite en mode WAL (connexions, notebooks, métriques, utilisateurs). Identifiants chiffrés (`aes-gcm`), mots de passe hachés (`argon2`).
- **Frontend** : Vue 3 + TypeScript + Vite, fichiers `.vue` avec `<script setup lang="ts">`, `vue-tsc`, Pinia, Vitest, CodeMirror 6 (npm), Reka UI (comportement des composants) et Tailwind CSS, icônes `lucide-vue-next`, ESLint + Prettier.
- **Types partagés** : les structures échangées entre Rust et l'interface sont définies une seule fois côté Rust, et leurs types TypeScript sont générés par `ts-rs`.
- **Livraison** : un seul exécutable, avec `web/dist` embarqué par `rust-embed`. Pas de service externe à l'exécution. Les dépendances npm et Cargo sont acceptées au développement.
- **Tests** : `testcontainers` pour MySQL, MariaDB et PostgreSQL, SQLite et DuckDB en mémoire.

Chaque crate Rust est présenté dans la section 3.2 de `docs/V1.md` (rôle, équivalent PHP, étape). Toute dépendance ajoutée à un `Cargo.toml` y est documentée dans le même changement.

## Organisation du dépôt

```
crates/        workspace Cargo
  types/       valeurs internes, nullabilité
  plan/        nœuds du plan logique
  dialects/    analyse et réécriture par dialecte : limite par défaut, lecture / écriture, puis catalogue de fonctions et émetteurs
  splitter/    découpage des requêtes par dialecte (Ctrl-Entrée)
  connectors/  interface commune et une implémentation par moteur
  server/      sessions de notebook, API, authentification
web/           application Vue
docs/          décisions et réflexions
```

En développement, Vite sert l'interface et redirige l'API vers `axum`. En production, `rust-embed` embarque `web/dist`.

## Git

Dérogation à la directive générale « lecture seule » : dans ce dépôt, Claude Code fait les commits.

- Un commit à chaque tâche réalisée.
- **Un commit = un motif de changement**, c'est-à-dire une seule raison de modifier le dépôt. Un motif peut toucher plusieurs fichiers. Deux raisons distinctes font deux commits, même dans un seul fichier : par exemple, une nouvelle règle de travail dans `CLAUDE.md` et la documentation des tests sont deux commits. Le type de fichier (documentation, code) ne définit pas le motif.
- Méthode : faire les modifications d'un seul motif, committer, puis passer au motif suivant. Avant chaque commit, relire `git diff --cached` : chaque ligne doit servir le motif annoncé par le message.
- Messages en français. Le sujet dit ce que fait le changement, sans préfixe de fichier, de crate ou de module (« README : … », « V1.md : … ») : les fichiers touchés se voient déjà dans le commit.
- Le sujet commence par un verbe conjugué à la 3e personne du singulier du présent (« Ajoute… », « Documente… », « Lit… »), jamais à l'infinitif.
- Toujours en lecture seule pour le reste : pas de push, pas de création ou suppression de branche, pas de réécriture d'historique.

## Commandes

À lancer à la racine du dépôt. La version de Rust est figée par `rust-toolchain.toml`.

```
cargo run                         # lance Prisme sur http://127.0.0.1:8484
cargo run -- --help               # options (--listen, --data-dir, --user-email)
cargo run -- --data-dir ./tmp     # dossier de données jetable, hors de %APPDATA%\Prisme
cargo test                        # tests de tous les crates, sans Docker
cargo test -- --include-ignored   # en plus, tests sur de vrais serveurs (Docker Desktop démarré)
cargo clippy --all-targets        # analyse statique, doit rester sans avertissement
cargo fmt                         # formatage
cargo build --release             # exécutable unique, interface embarquée (après npm run build)
```

Dans `web/` :

```
npm run dev                       # interface sur http://localhost:5173 (avec cargo run à côté)
npm run test:unit -- --run        # tests Vitest
npm run type-check                # vue-tsc
npm run lint                      # Oxlint et ESLint
npm run format                    # Prettier
npm run build                     # interface compilée dans web/dist, embarquée par rust-embed
```

`cargo test` régénère les types TypeScript de `web/src/api/generated/` (`ts-rs`) : les committer avec le changement Rust qui les modifie.

Journaux plus détaillés : `$env:RUST_LOG="debug"` (PowerShell) avant `cargo run`. Les tests qui demandent Docker sont marqués `#[ignore = "nécessite Docker Desktop démarré"]`. Toujours lancer `cargo fmt` et `cargo clippy --all-targets` avant un commit, et pour l'interface `npm run format`, `npm run lint`, `npm run type-check` et les tests.

## Règles à respecter

1. **MySQL et MariaDB sont deux dialectes distincts**, avec une base commune. La version du serveur est détectée à la connexion (`SELECT VERSION()`, qui contient « MariaDB » pour ce dernier).
2. **Jamais de traduction approximative silencieuse.** Une fonction se traduit de façon native, par une expression équivalente (émulée), ou, en dernier recours, par une erreur explicite. L'émulation doit conserver le type du résultat et l'utilisation des index. L'aperçu de la requête traduite signale les émulations.
3. **Limite de lignes** : si l'utilisateur n'a écrit aucune limite, l'application ajoute `LIMIT N+1` (au niveau de l'AST), affiche N lignes et signale « résultat tronqué ». Si l'utilisateur a écrit une limite, elle passe telle quelle, quelle qu'elle soit. Lecture toujours en streaming.
4. **Interrupteurs d'écriture** (UPDATE, DELETE, CREATE, DROP, ALTER) : classification par l'AST, jamais par le texte, plus un compte de connexion en lecture seule ou une transaction `READ ONLY` côté base quand le moteur le permet.
5. **Les NULL suivent la sémantique SQL** (logique à trois valeurs, sacs et non ensembles). La nullabilité des colonnes est portée par les types et par le plan dès le départ.
6. **CSV : tout est texte**, sans inférence de type. Le typage passe par les `CAST` écrits par l'utilisateur.
7. **Aucun identifiant de connexion en clair**, ni dans la base interne, ni dans les journaux.
8. **Suite de conformité** : une même requête est exécutée sur chaque base et les résultats sont comparés. Toute règle de traduction ajoutée doit avoir son test.
9. **En cas de doute sur une décision de conception, demander à Sébastien.** Les points ouverts sont listés dans `docs/QUESTIONS.md`. Ne pas trancher en silence.
10. **Interface : tout passe par les variables CSS du thème** (`web/src/assets/main.css`) : couleurs par rôle, tailles de police, espacements intérieurs et extérieurs, hauteurs, arrondis. Pas de valeur en dur dans un composant quand une variable existe ou a lieu d'exister. Les longueurs sont relatives (`rem`, `em`) ; seuls les traits (bordures, contour du focus) restent en pixels. Chaque couleur a sa valeur claire et sombre ; tout nouvel écran se vérifie dans les deux modes.

## Ordre de développement de la v1

1. Workspace, base interne, connexions chiffrées, connecteur MySQL/MariaDB, requête en streaming avec limite par défaut.
2. Interface minimale : explorateur d'objets, cellule SQL, résultats paginés.
3. Notebook, découpeur, Ctrl-Entrée et Ctrl-Maj-Entrée, onglets de résultats, exécution successive.
4. Transactions, annulation, exécution parallèle, interrupteurs d'écriture.
5. PostgreSQL et SQLite, puis traduction entre dialectes avec la suite de conformité.
6. Métriques, DuckDB, puis Tutorial D (sous-ensemble d'abord).

Détail et risques de chaque étape dans `docs/V1.md`.
