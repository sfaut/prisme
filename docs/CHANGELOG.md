# Changelog

Changelogs des versions publiées de Prisme, la plus récente en haut. Chaque entrée est aussi la note de sa release GitHub.

## v0.2.0 (2026-10-16)

### 💥 Changement incompatible

- L'option courte qui affiche la version devient `-v` ; `-V` n'est plus reconnue, `--version` ne change pas ([#7](https://github.com/sfaut/prisme/issues/7)).

### ✨ Nouvelles fonctionnalités

- Réglages au premier lancement : Prisme demande dans le terminal le port d'écoute (8484 par défaut) et l'adresse e-mail de l'utilisateur (`me@local` par défaut), les enregistre dans sa base interne, et ne les redemande plus. `prisme --setup` repose les questions ; `--listen` et `--user-email` ne valent que pour un lancement. Une installation existante garde ses réglages, sans question ([#6](https://github.com/sfaut/prisme/issues/6)).
- Éditeur SQL : Tab avance jusqu'au taquet suivant (tous les quatre espaces) ou indente les lignes sélectionnées, Maj-Tab les désindente ; Échap puis Tab, ou Ctrl-M, quittent l'éditeur au clavier ([#2](https://github.com/sfaut/prisme/issues/2)).
- Grille de résultats : les colonnes `CHAR` s'affichent en chasse fixe ([#4](https://github.com/sfaut/prisme/issues/4)), et le marqueur `NULL` en petites capitales, plus discret ([#1](https://github.com/sfaut/prisme/issues/1)).

### 🐞 Corrections de bugs

- Éditeur SQL : Entrée n'ajoute plus deux espaces en début de ligne ; le curseur va en première colonne ([#5](https://github.com/sfaut/prisme/issues/5)).
- Copie de la grille (Ctrl-C) : les valeurs sont copiées telles quelles, sans guillemets ajoutés ; les caractères de contrôle d'une valeur sont écrits en toutes lettres (`\t`, `\n`, `\r`…), pour garder une valeur par cellule ([#3](https://github.com/sfaut/prisme/issues/3)).

### ⚙️ Optimisation interne

- Dépendances à jour : Rust 1.99.1, Tokio 1.53.3, Vitest 5, Oxlint 1.88, Prettier 3.9.10, et les actions GitHub de l'intégration continue.

### 📝 Documentation

- README en anglais, pour l'utilisateur final, avec un démarrage rapide ; guide du développeur à part, dans `docs/DEVELOPER.md`.
- Licence MIT : titulaire des droits précisé.

## v0.1.0 (2026-10-15)

### 🚀 Lancement

Première version publique de Prisme, client web multi-bases de données en forme de notebook SQL. Cette v0.1.0 interroge MySQL et MariaDB ; PostgreSQL, SQLite, DuckDB et la traduction entre dialectes viendront ensuite.

- Un seul exécutable par plateforme, interface comprise : Windows (x86_64), Linux (x86_64) et macOS (universel, Apple Silicon et Intel). Rien d'autre à installer.
- Prisme se lance en local et s'ouvre dans le navigateur, sur <http://127.0.0.1:8484>.
- Les exécutables ne sont pas encore signés : Windows affiche un avertissement SmartScreen au premier lancement, et macOS demande de lever la quarantaine du fichier (procédure dans le README).

### 🔒 Sécurité

- Mots de passe des connexions chiffrés dans la base interne (AES-256-GCM), jamais écrits dans les journaux ni renvoyés par l'API.
- Écoute limitée à la machine locale : sans authentification, Prisme refuse d'être joignable depuis le réseau.
- Lecture seule : les sessions de requêtes refusent les écritures (`UPDATE`, `DELETE`, DDL, `CALL`…), détectées par l'analyse de la requête, en attendant les interrupteurs d'écriture.

### ✨ Nouvelles fonctionnalités

**Connexions**
- Ajout, modification, test et suppression des connexions MySQL / MariaDB, avec détection de la variante et de la version du serveur, affichées en badge aux couleurs du moteur.
- Requêtes exécutées à l'ouverture de chaque connexion (`SET NAMES`, `sql_mode`…).
- Messages clairs en cas d'échec de connexion.

**Explorateur d'objets**
- Sources, bases, tables, vues, procédures, fonctions ; colonnes, index, clefs étrangères et déclencheurs de chaque table.
- Catalogue affiché au fil de l'eau, colonnes alignées avec leur type, clef primaire, clefs étrangères, colonnes indexées, `UNIQUE` et `AUTO_INCREMENT` signalés.
- Double clic : choisir une source, changer de base en cours, afficher le contenu d'une table.
- Bases système masquables, panneau redimensionnable.

**Éditeur SQL**
- Plusieurs éditeurs en onglets, chacun avec sa connexion et sa session, enregistrés avec leurs résultats et rouvrables 24 heures après fermeture.
- Ctrl-Entrée exécute la requête sous le curseur, surlignée dans l'éditeur ; Ctrl-Maj-Entrée exécute toutes les requêtes en parallèle, une par onglet de résultat.
- Complétion des bases, tables et colonnes, erreurs de syntaxe soulignées avant l'exécution.
- Bouton « Enjoliver » : mots-clefs et fonctions en capitales, noms dans l'écriture du catalogue, `AS` devant les alias, mise en page par clauses, blancs invisibles nettoyés, procédures stockées et `DELIMITER` pris en charge.
- Bouton « Stopper » pour arrêter les requêtes en cours.

**Résultats**
- Lecture en streaming, avec une limite de 1 000 lignes par défaut (réglable jusqu'à 50 000) quand la requête n'en précise aucune, et la mention « résultat tronqué ».
- Onglets de résultat épinglables, renommables et déplaçables, avec pastille d'avancement, durées d'exécution et de transfert, volume et débit reçus.
- Grille compacte : tri, filtre, colonnes déplaçables et redimensionnables, type de chaque colonne dans l'en-tête, nombres et dates alignés à droite, liens web cliquables, JSON et XML reconnus.
- Sélection à la souris ou au clavier, copie par Ctrl-C avec les noms de colonnes, collable dans un tableur.

**Interface**
- Thème clair, sombre ou celui du système, éditeur compris.
- Barre de partage entre l'éditeur et les résultats.

### 📝 Documentation

- README : installation, lancement, utilisation de l'interface et premier lancement des exécutables non signés.
