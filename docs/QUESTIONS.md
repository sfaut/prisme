# Pistes explorées, décisions et questions ouvertes

Ce document garde la trace du raisonnement : ce qui a été retenu, écarté ou reporté, et pourquoi, plus les questions encore ouvertes. Il évite de rouvrir des débats déjà tranchés.

## 1. Chronologie de l'évolution du projet

1. **Idée de départ** : un client qui se connecte à plusieurs sources (serveurs SQL, fichiers CSV), met en forme les données comme un client MySQL, avec un dialecte agnostique de la source.
2. **Problème à résoudre** : la performance. DuckDB croise mal plusieurs sources. Il faut un moteur qui analyse la requête, va chercher les données de façon optimale dans chaque source, puis assemble.
3. **Première orientation technique** : zéro dépendance, tout en PHP (stack habituelle de Sébastien : PHP 8.5, Vue 3, MySQL 8, DuckDB, Git).
4. **Extensions** : dialecte neutre sur base ANSI, autres langages d'entrée (syntaxe pipe, PRQL), matérialisation des résultats dans un workspace, écritures, rôles, valeurs imbriquées, théorie de Date et Tutorial D, NULL, CSV.
5. **Nom** : Prisme (voir la section 4).
6. **Recentrage de la v1** : un client multi-bases en notebook, chaque requête vers une seule base, avec traduction entre dialectes. Le moteur fédéré passe après.
7. **Choix de la stack** : Rust, puis frontend Vue 3 + TypeScript + Vite « selon l'industrie ».

## 2. Pistes retenues

- **Pipeline à plan logique pivot** : frontends vers plan, un optimiseur écrit une fois, émetteurs par source.
- **Un langage par requête**, choisi par l'utilisateur.
- **ANSI strict comme noyau** du dialecte neutre, enrichi par emprunts (DuckDB, ClickHouse).
- **Noms explicites pour les doublons** : `UnionAll`, `UnionDistinct`, `MinusAll`, `MinusDistinct`, `IntersectAll`, `IntersectDistinct`.
- **Théorie de Date** pour le plan : `Group` et `Ungroup` comme nœuds, `SemiJoin` et `AntiJoin` de première classe.
- **Tutorial D en deux modes** : flex (sémantique SQL) en priorité, strict reporté.
- **Sacs et NULL conservés** en interne.
- **CSV tout en texte**, casts par l'utilisateur, option de NULL sur le scan, lecteur qui garde l'information de guillemets.
- **Workspace SQLite** par utilisateur ou session, cache jetable, enregistrement explicite.
- **Une source lue, une destination écrite.**
- **Limite par défaut** ajoutée par l'application, limite de l'utilisateur prioritaire quelle qu'elle soit.
- **Traduction à trois niveaux** (native, émulée, non supportée en dernier recours), avec aperçu.
- **Rust** pour tout, dont l'exécuteur fédéré futur ; **pilotes natifs**.
- **Vue 3 + TypeScript + Vite**, types générés depuis Rust.
- **Suite de conformité** multi-bases comme filet de sécurité.
- **Interface de stockage et de connecteurs abstraite**, pour pouvoir changer de moteur de workspace plus tard.

## 3. Pistes écartées ou reportées

| Piste | Statut | Raison |
|---|---|---|
| PHP pur, zéro dépendance pour tout le moteur | Écartée | Le parsing est rapide en PHP, mais les tableaux PHP coûtent cher en mémoire, les jointures locales sont lentes, et il n'y a pas de pilote DuckDB natif. Rust est la cible. À noter : le parseur, l'optimiseur et l'orchestration étaient jugés faisables en PHP, et le parallélisme d'I/O possible (`MYSQLI_ASYNC`, `proc_open`). |
| Python + FastAPI + SQLGlot pour la v1 | Écartée | Recommandée pour la rapidité de livraison (SQLGlot traduit entre dialectes), mais Sébastien a préféré un seul langage, Rust. SQLGlot reste la référence de conception du transpileur. |
| Go | Écartée | Bon pour les sessions longues, mais écosystème de parseurs fragmenté et rien d'équivalent à DataFusion pour le moteur fédéré. |
| Frontend sans build (JavaScript + JSDoc + `@ts-check`, Vue en fichier copié) | Écartée | Sébastien tient aux types et à « l'industrie ». Navigateurs : pas de TypeScript natif (TC39 type annotations au stade 1). Sans build, les templates ne sont pas typés et presque aucune bibliothèque d'interface ne convient. |
| DataFusion, Calcite, Trino comme base du moteur | Reportée | Sébastien veut éviter les dépendances. Leur modèle de plan sert de référence. DataFusion reste une option pour l'exécuteur. |
| PrimeVue comme bibliothèque de composants | Écartée | Licence commerciale à clef depuis la v5 (juillet 2026), même en version communautaire gratuite. La v4.5.5, dernière sous MIT, est figée. Remplacée par Naive UI (MIT). |
| `sqlx` pour les accès aux bases | Écartée | Gère mal les résultats dynamiques sans schéma connu, et l'annulation est moins directe. |
| Tri déterministe ajouté d'office | Écartée | Sébastien sait que sans `ORDER BY` l'ordre n'est pas garanti. |
| Typage ou schéma déclaré par fichier CSV | Écartée | Compliquerait l'outil. Casts dans la requête. Seule la déclaration de source (chemin, séparateur, encodage, en-tête) est gardée. |
| Inférence de types dans les CSV (booléens, vide = NULL, guillemets = texte) | Écartée | Pas de consensus entre outils, les guillemets ne portent pas de sens fiable, risque de faux positifs. |
| Mode « guillemets = chaîne vide, vide nu = NULL » | Reportée | Réalisable si le lecteur garde l'information de guillemets. |
| Écritures vers plusieurs destinations dans une requête | Écartée | Aucun cas d'usage, complexité et non-atomicité. |
| Mélange de langages dans une même requête | Écartée | Un langage par requête suffit. |
| Réutilisation automatique d'un résultat matérialisé (query matching) | Écartée en v1 | Problème difficile. Enregistrement explicite sous un nom. |
| Parquet pour le workspace | Écartée | Écriture lourde. SQLite avec index est le meilleur compromis. |
| `UPDATE` et `DELETE` sur CSV | Reportée | Réécriture complète du fichier et verrou ; seulement si un besoin réel apparaît. |
| Mode strict de Tutorial D | Reportée | Non critique. Seule la nullabilité doit exister dès maintenant. |
| Vrai système de base de données maison | Option future | Sébastien n'exclut pas d'y aller plus tard. |
| Moteur fédéré | Après la v1 | La v1 construit le noyau commun. |

## 4. Nom du projet

- Retenu : **Prisme** (une seule vue sur plusieurs sources). Le nom de travail précédent était « Mineur ».
- Variantes proposées puis écartées : Pivot, Espéranto, Carrefour, Mineur (ambigu en français, évoque le *data mining*), Prisql, Prysme, Prismo, Prisme/QL, `prism.sh`, `prisme.dev`.
- À éviter : **Prisma** (ORM très connu) et **Prism** tout court (déjà pris). Vérifier la disponibilité du domaine, du dépôt et d'une éventuelle marque avant de publier.

## 5. Questions ouvertes

### Produit et v1

1. **Grille de résultats** : besoins fixés (affichage compact, tri, filtre, sélection, copie). Proposition : composant propre avec `@tanstack/vue-virtual` (voir `docs/V1.md`, section 3.1). **[à confirmer]** Essai de volumétrie à faire.
2. **`ts-rs` ou `specta`** pour générer les types TypeScript : tranché, `ts-rs`, plus simple à relire et suffisant.
3. **MariaDB dans `sqlparser-rs`** : vérifier le support, sinon étendre le dialecte MySQL.
4. **DDL** : le laisser en dialecte natif en v1 (proposition) ? **[à confirmer]**
5. **Métriques de la v1** : lesquelles précisément (taille, nombre de lignes estimé, cardinalité, taux de NULL, statistiques par colonne) ? Et à quelle fréquence les rafraîchir ?
6. **Authentification en v1** : tranché. La v1 cible le poste de développement (un utilisateur, `sebastien.faut@gmail.com`, sans authentification, écoute sur `127.0.0.1`) et doit évoluer facilement vers un serveur multi-utilisateur. Les rôles viennent après la v1 (voir `docs/V1.md`, section 2.8).
7. **Persistance du notebook** : cellules, ordre, et résultats conservés ou recalculés ?
8. **Dialectes versionnés** : la version est détectée à la connexion ; que change-t-elle (coloration, autocomplétion, avertissements de traduction) ?
9. **Parallèle et transactions** : exécution parallèle sur des connexions distinctes, donc hors de la transaction de la session. Comportement et message à fixer.
10. **Tutorial D en v1** : placé en dernier, en sous-ensemble sur une seule source. Convient-il ?
11. **Langue du code** : tranché, identifiants en anglais, documentation en français.

### Exécution

24. **Requêtes qui changent la session dans un lot parallèle** : en Ctrl-Maj-Entrée, chaque requête a sa propre connexion, donc un `USE` ou un `SET` du lot ne touche pas les autres requêtes (`USE xlist; SELECT * FROM video;` lit `video` dans la base en cours, pas dans `xlist`). Décision provisoire de Sébastien : exécution entièrement parallèle, à affiner. Piste : exécuter d'abord, une par une, les requêtes qui changent la session, puis les autres en parallèle.

25. **Tests de bout en bout dans un vrai navigateur** (Playwright, avec Edge ou Chrome déjà installés) : scénarios de fonctionnement (connexions, explorateur, exécution sous le curseur et en parallèle, « Stopper », « Enjoliver », grille) et garde-fous de performance (nombre de lignes dessinées, délai d'un clic), sur un environnement isolé (MariaDB dans Docker, Prisme et Vite sur d'autres ports). Les tests dans jsdom ne voient ni la mise en page ni les performances d'affichage (voir le chapitre 1 de `docs/PERF.md`). Décision de Sébastien : à voir plus tard. À trancher alors : à quel moment les lancer (avant chaque commit qui touche l'interface, ou à la fin de chaque étape).

26. **Reconstruction partielle de l'arbre de l'explorateur.** L'arbre entier se reconstruit à chaque révision du catalogue (au plus 0,4 s sur 1 000 tables, voir le chapitre 2 de `docs/PERF.md`). Avec beaucoup de sources et beaucoup de tables, colonnes, index, il faudra ne reconstruire que les bases modifiées, voire construire les nœuds seulement à l'ouverture. Décision de Sébastien : à faire plus tard.

27. **Copie avec ou sans en-têtes, réglage du profil.** Ctrl-C copie la sélection de la grille avec les noms des colonnes en première ligne. Décision de Sébastien : ce sera une option du profil de l'utilisateur, avec les en-têtes par défaut. À faire avec les profils (mode serveur, ou réglages de l'utilisateur unique).

### Sécurité

23. **Clef maîtresse du chiffrement** : tranché, fichier de clef `prisme.key` dans le dossier de données (voir `docs/V1.md`, section 2.8).

### Moteur fédéré

12. **NULL** : les 12 points de la section 7 de `docs/FEDERATED.md` sont à trancher, en priorité les points 1 à 6.
13. **Collations** : quelle règle unique pour comparer les chaînes ?
14. **Fuseaux horaires** : quelle convention unique ?
15. **`NOT IN` avec NULL** : reproduire le comportement SQL tel quel ou traduire en `AntiJoin` ?
16. **Sémantique des frontends PRQL et pipe** : héritent-ils de la sémantique de référence ou gardent-ils la leur ?
17. **« PSQL »** : s'agit-il bien de la syntaxe SQL en pipe ? **[à confirmer]**
18. **Types de la v1 du moteur** : liste d'abord, ensemble ou relation imbriquée ensuite ? Comportement de `Ungroup` sur liste NULL ou vide, et de `array_agg` sur les NULL.
19. **Types entiers hors plage** : bibliothèque de grands entiers, ou erreur dans tous les cas ?
20. **Besoin d'écritures atomiques entre sources** : écarté pour le moment, à réexaminer si un cas d'usage apparaît.
21. **Écriture de NULL dans un CSV** (vide, `\N`, `NA`) : option à prévoir pour `Export`.
22. **Mode strict de Tutorial D** : transformer les données (valeurs par défaut déclarées par source) ou seulement contrôler ?

## 6. Références et sources

Plusieurs références ci-dessous sont citées de mémoire : à vérifier avant de s'y appuyer.

- C. J. Date et Hugh Darwen, *The Third Manifesto* ; C. J. Date, « What First Normal Form Really Means » (références exactes à vérifier).
- Tutorial D : implémentations Rel (Java) et Dee (Python).
- RFC 4180 (CSV, informative, 2005) et W3C « CSV on the Web » (CSVW, propriété `null`).
- SQLGlot (Python), référence de conception pour un transpileur de dialectes.
- `sqlparser-rs`, Apache DataFusion (dont l'« unparser » plan vers SQL), Apache Calcite, Trino.
- DuckDB : extensions pour MySQL, PostgreSQL, SQLite, CSV et Parquet.
- PRQL, et la syntaxe SQL en pipe.
- Manticore et Sphinx : origine du terme « MVA ».
- TypeScript dans les navigateurs : proposition TC39 « type annotations » au stade 1 ; Node exécute TypeScript en supprimant les types (serveur seulement). Source : [A Small Year for tsc, a Giant Year for TypeScript](https://effectivetypescript.com/2025/12/19/ts-2025/) (décembre 2025).
- Claude Code : [Quickstart](https://code.claude.com/docs/en/quickstart).
