# Gel de l'interface pendant l'exécution : la grille dessinait toutes les lignes

Ce document décrit un problème de performance rencontré en octobre 2026 dans
la grille de résultats : ce qui se voyait, comment il a été mesuré, pourquoi
il est apparu et comment il a été corrigé. Il sert aussi de mode d'emploi
pour mesurer à nouveau.

## 1. Symptôme

Pendant l'exécution de plusieurs requêtes en parallèle (Ctrl-Maj-Entrée), les
onglets de résultat se figeaient : un clic sur un onglet mettait plusieurs
secondes à être pris en compte, et toute l'interface semblait bloquée jusqu'à
la fin de l'exécution.

Une première correction (commit `6176cca`) a regroupé les messages reçus du
serveur pour redessiner l'interface au plus une fois toutes les 100 ms. Elle
n'a rien changé de visible : elle visait un coût réel, mais secondaire, et
elle a été écrite sans mesure.

## 2. Mesure

Le problème a ensuite été mesuré dans un vrai navigateur :

- environnement isolé : MariaDB 11.3 dans Docker, un Prisme et un serveur Vite
  sur d'autres ports et un dossier de données jetable, pour ne pas toucher à
  l'instance de développement ;
- Edge, sans fenêtre, piloté par Playwright (`playwright-core`, qui utilise
  le navigateur installé) ;
- scénario : 8 requêtes en parallèle, dont 4 de 50 000 lignes et 4 de 1 250
  lignes, puis 12 clics sur des onglets pendant l'exécution ;
- indicateurs : délai entre le clic et l'onglet affiché, « tâches longues »
  du navigateur (blocages de plus de 50 ms, relevés par un `PerformanceObserver`),
  profil CPU (`Profiler` du protocole Chrome DevTools), nombre d'éléments
  présents dans la page.

| Mesure | Avant | Après la 1re correction (`a91fd29`) | Après la 2e correction (`d0534f6`) |
|---|---|---|---|
| Durée totale de l'exécution | 118 s | 6 s | 4,5 s |
| Délai d'un clic sur un onglet | 6 à 22 s | 0,07 à 0,5 s | 0,04 à 0,18 s |
| Tâches longues (nombre, total) | 65, 110 s | 13, 1,2 s | 2, 0,14 s |
| Plus longue tâche | 13,5 s | 131 ms | 78 ms |
| Éléments dans la page (2 résultats de 20 000 lignes) | 100 400 | 548 | 548 |

Le profil d'avant montrait 53 % du temps dans le moteur du navigateur (calcul
des styles et de la mise en page), 14 % de ramasse-miettes, et le reste dans
le rendu des lignes de la grille. Le nombre d'éléments a donné la clef : la
grille d'un seul onglet contenait 20 000 lignes dessinées, pour un résultat de
20 000 lignes.

## 3. Cause principale : le défilement virtuel était contourné

### 3.1 Le principe du défilement virtuel

La grille de résultats (`web/src/components/ResultGrid.vue`) affiche des
dizaines de milliers de lignes en n'en dessinant que quelques dizaines : celles
qui sont visibles dans sa zone de défilement, plus une marge. La bibliothèque
`@tanstack/vue-virtual` calcule ces lignes à partir de la hauteur de la zone
de défilement.

```text
 Fonctionnement attendu (zone de défilement de 600 px, lignes de 22 px)

 ┌──────────────── zone de défilement : 600 px ───────────────┐
 │ ligne 1 040   ◄── dessinée                                 │
 │ ligne 1 041   ◄── dessinée                                 │
 │ …             ◄── ~27 lignes visibles + 20 de marge        │
 │ ligne 1 067   ◄── dessinée                                 │
 └────────────────────────────────────────────────────────────┘
   lignes 1 à 1 019 et 1 088 à 20 000 : seulement un espace vide
   de la bonne hauteur, pour que la barre de défilement soit juste
```

### 3.2 Ce qui se passait

Le calcul suppose que la zone de défilement a une hauteur **bornée** par son
conteneur. Or rien ne la bornait : elle prenait la hauteur de son contenu. Pour
20 000 lignes de 22 pixels, elle mesurait 440 022 pixels. Pour la
bibliothèque, toutes les lignes étaient donc « visibles », et elle les
dessinait toutes.

```text
 Avant la correction                         Après la correction

 fenêtre (900 px)                            fenêtre (900 px)
 ┌───────────────────────────┐               ┌───────────────────────────┐
 │ disposition : 274 px      │               │ disposition : 900 px      │
 │ ┌───────────────────────┐ │               │ ┌───────────────────────┐ │
 │ │ éditeur, boutons      │ │               │ │ éditeur, boutons      │ │
 │ │ onglets               │ │               │ │ onglets               │ │
 │ │ ┌───────────────────┐ │ │               │ │ ┌───────────────────┐ │ │
 │ │ │ grille            │ │ │               │ │ │ grille : 640 px   │ │ │
 │ │ │ 440 022 px        │ │ │               │ │ │ (place restante)  │ │ │
 │ │ │ = 20 000 lignes   │ │ │               │ │ │ ~30 lignes        │ │ │
 │ │ │   dessinées       │ │ │               │ │ │   dessinées       │ │ │
 ┆ ┆ ┆                   ┆ ┆ ┆               │ │ └───────────────────┘ │ │
 ┆ ┆ ┆  (déborde de la   ┆ ┆ ┆               │ └───────────────────────┘ │
 ┆ ┆ ┆   page)           ┆ ┆ ┆               └───────────────────────────┘
```

Chaque lot de lignes reçu redessinait alors des dizaines de milliers
d'éléments, dans chaque onglet ouvert. Le navigateur passait son temps à
recalculer styles et mise en page, et ne traitait plus les clics.

### 3.3 Pourquoi c'est arrivé

Trois éléments se sont combinés :

1. **La disposition ne remplissait pas la fenêtre.** Le composant `NLayout` de
   Naive UI, utilisé pour la disposition générale (`web/src/App.vue`), prend
   par défaut la hauteur de son contenu, et non celle de la fenêtre. L'écran
   ne mesurait que 274 pixels de haut : il n'existait donc aucune « place
   restante » à partager.
2. **La chaîne des hauteurs était rompue.** Pour qu'un élément prenne la place
   restante sans la dépasser, en CSS, chaque niveau doit être un conteneur
   flexible (`display: flex`) avec `flex: 1` et `min-height: 0`. Sans
   `min-height: 0`, un élément flexible ne rétrécit jamais sous la hauteur de
   son contenu. Les onglets de résultat (`NTabs`), ajoutés pour l'exécution
   parallèle, ont introduit des niveaux de plus dans cette chaîne, sans ces
   règles.
3. **Rien ne le signalait.** Les tests de l'interface tournent dans jsdom, un
   navigateur simulé sans moteur de mise en page : toutes les hauteurs y
   valent 0, et une grille non bornée n'y coûte rien. Le défaut ne se voit
   qu'avec beaucoup de lignes, dans un vrai navigateur. Avec une requête de
   1 000 lignes (la limite par défaut), il passait presque inaperçu.

## 4. Cause secondaire : la réactivité sur chaque ligne

Une fois la grille bornée, le profil a montré un autre coût : 11 % du temps
dans le suivi des changements de Vue (`set`). Les lignes reçues étaient
ajoutées une à une dans un tableau réactif, et Vue interceptait chaque ajout,
alors que seul leur nombre importe pour redessiner la grille.

## 5. Corrections

| Commit | Correction |
|---|---|
| `a91fd29` | La disposition remplit la fenêtre (`position="absolute"` sur `NLayout`). Chaque niveau, jusqu'à la grille, prend la place restante sans la dépasser (`flex: 1`, `min-height: 0`). **Garde-fou** : la zone de défilement de la grille ne peut pas dépasser la hauteur de la fenêtre (`max-height: 100vh`), pour que le défilement virtuel ne puisse plus être contourné, même si la chaîne des hauteurs est rompue à nouveau. |
| `d0534f6` | Les lignes reçues sont gardées dans un tableau ordinaire (`markRaw`), hors du suivi de Vue. Seul leur nombre (`rowCount`) est réactif : la grille et les compteurs le lisent pour se redessiner. |
| `6176cca` | Conservé : les messages du serveur sont appliqués par paquets, toutes les 100 ms, et l'horloge des durées ne redessine que les pastilles des requêtes en cours. |

## 6. Ce qu'il faut retenir

- **Une grille virtualisée a besoin d'une hauteur bornée.** Tout nouveau
  conteneur placé entre la disposition et la grille doit prolonger la chaîne
  `flex: 1` / `min-height: 0`. Le garde-fou `max-height: 100vh` limite les
  dégâts, mais ne remplace pas une chaîne correcte.
- **Mesurer avant de corriger.** La première correction, faite sans mesure, a
  traité un coût secondaire. La mesure dans un vrai navigateur a désigné la
  cause en quelques minutes : le nombre d'éléments dans la page.
- **Les tests dans jsdom valident la logique, pas l'affichage.** Pour les
  performances d'affichage, seul un vrai navigateur fait foi.

## 7. Mesurer à nouveau

1. Démarrer un MariaDB jetable : `docker run --rm -d --name prisme-perf -p 3310:3306 -e MARIADB_ROOT_PASSWORD=secret -e MARIADB_DATABASE=test mariadb:11.3`.
2. Lancer un Prisme isolé : `cargo run -- --listen 127.0.0.1:8599 --data-dir ./tmp/perf`, puis y enregistrer une connexion vers `127.0.0.1:3310` (compte `root`, base `test`).
3. Lancer Vite sur un autre port, avec un proxy vers `127.0.0.1:8599`.
4. Avec Playwright (`playwright-core`, navigateur `msedge` ou `chrome` déjà installé) : ouvrir l'interface, choisir la connexion, saisir plusieurs requêtes sur `seq_1_to_200000` (table de séquence de MariaDB), cliquer sur « Tout exécuter », puis cliquer sur les onglets pendant l'exécution en mesurant le délai, et relever les tâches longues avec un `PerformanceObserver` de type `longtask`.
5. Indicateur le plus parlant : `document.querySelectorAll('.grid-row').length` pendant l'exécution. Il doit rester de l'ordre de quelques dizaines par onglet visible, quel que soit le nombre de lignes reçues.
