# Prisme: design brief for Claude Design

This document is the prompt given to Claude Design. It describes the interface of Prisme as it stands on 2026-10-28 (v0.7.0 and the commits after it), the rules any design keeps, and what is expected. It is pasted whole; screenshots of the current screen, light and dark, go with it when available.

## 1. The product

Prisme is a multi-database client shaped like a SQL notebook: several sources, one view. A single executable runs on the user's computer and opens in the browser, on a desktop screen, used with mouse and keyboard, often at a Windows scale of 150%. Today it queries MySQL and MariaDB; PostgreSQL, SQLite, DuckDB and translation between SQL dialects come next.

Its users write SQL all day: developers, database administrators, data analysts. The interface is a dense working tool, in the family of DataGrip, DBeaver, TablePlus or DuckDB UI: a lot of data on screen, small text, quiet chrome, the content first (SQL, grids, trees).

The interface is in French. Each label below is quoted as it appears on screen, with a translation in parentheses; the designs keep the French labels.

## 2. What is expected

1. **Reference**: a faithful reproduction of the main screen described in sections 5 to 7, light and dark, filled with the sample data of section 9. It is the base the proposals are compared with.
2. **Proposals**: variants that improve the design within the rules of section 3: visual hierarchy, consistency between the areas, legibility of the dense parts (tree, grid, console), clarity of the states (running, error, open transaction, empty). Each variant sits beside the reference with a short note: what changes, and why. The areas and the features stay; a variant that moves, merges or removes an element says so explicitly.
3. **States**: the situations of section 8, one frame each.

Output: HTML with Tailwind CSS v4 utility classes naming the roles of section 4 (`bg-surface`, `text-fg-secondary`, `border-line`...), and Lucide icons named as in `@lucide/vue`. The interface is written in Vue 3, with Reka UI for the behaviour of the components (tabs, tree, menus, lists, windows, splitters, toasts) and Tailwind for their rendering: the designs are ported by hand, so plain markup and utility classes port directly. A component library (shadcn/ui, MUI...), CSS-in-JS or inline style values would have to be rewritten.

## 3. Rules of the design

These rules are those of the code: a design that breaks one cannot be ported as it is.

- **One theme**: every color, size and stroke comes from the theme (section 4), so that changing a few root values changes the whole interface. No hard-coded value in a component.
- **Scales**: Tailwind v4's default scales, unchanged, set by the font size of the root: `html { font-size: 75% }`. At 75%, `text-base` (1rem) is the regular text of the interface, and the spacing step (`--spacing`) is a quarter of it. No custom scale, no class or variable that doubles a Tailwind one.
- **Lengths**: whole steps of the spacing scale (`gap-2`, `p-3`, `h-8`, `w-40`, `size-5`), never a fraction (`gap-1.5`), never a pixel value. Strokes are the only lengths in pixels: borders and focus outline 1px, insertion markers 2px, progress bar of a result tab 2px.
- **Lines**: a line is always a border (`border-*`) or an outline. A thin box filled by its background (`w-px bg-...`) is painted one or two pixels thick at 125% or 150%, depending on its position.
- **Space between elements**: given by the container, with `gap` and padding, aligned with `justify-*` and `items-*`. Flexbox's own margins (`ms-auto`, `m-auto`) only when no container can align. No negative margins, no overlap: each element draws its own borders.
- **Heights**: natural, from the font and the padding, for bars, rows and tabs. The controls (button, field, list, menu item) are the exception, with two fixed sizes: regular, `h-8`, `text-base`, `px-3`, `gap-3`; small, `h-7`, `text-sm`, `px-1`, `gap-1`. The small size changes height, text, padding and gap together.
- **Icons**: Lucide, a single size, `size-5`, small buttons included. Text marks are the only `size-4` icons: chevrons of the tree, type icons of the grid's headers, link icon in a cell.
- **Colors**: the roles of section 4 only, one class per property. Shades go through opacity modifiers (`bg-primary/15`, `border-error/50`). Each role carries its light and dark values (`light-dark()`), so a component never stacks `dark:` variants.
- **Focus**: text input fields alone show a focus ring (1px outline, `primary` at 60%, offset 1px). Buttons, tabs, tree, grid, menus and the SQL editor show none.
- **Text centered on its capitals**: the label of a button, a badge or a tree line uses a `text-trim` utility (`text-box: trim-both cap alphabetic; padding-block: 0.25em`), which puts the text in the middle of its box, level with its icon.
- **Fonts**: Tailwind's `font-sans` (the system font, Segoe UI on Windows) for the interface; `font-mono` (`ui-monospace, 'Cascadia Code', Consolas, monospace`) for the SQL editor, the gutters, `CHAR` columns, the data explorer and the SQL of the console.
- **Text sizes in use**: `text-2xl` the name of the application; `text-lg` titles of panels and windows, text of the code editors; `text-base` regular text, regular controls, grid cells; `text-sm` small controls, status bars, console tables, gutters, notes; `text-xs` version number, badges.
- **Radii and shadows**: `rounded-lg` controls, top corners of tabs, code editors, banners, tooltips; `rounded-xl` menus, lists, windows, toasts; `rounded-full` avatar and badges. `shadow-lg` menus, lists, toasts; `shadow-xl` windows. No other shadow.
- **Light and dark**: every screen exists in both modes. The mode follows the system, or the choice of the top bar.

## 4. Theme

Colors by role, drawn from Tailwind's palette, declared in `@theme` (Tailwind turns each one into classes: `bg-surface`, `text-fg`, `border-line`, `accent-primary`...).

| Role | Use | Light | Dark |
|---|---|---|---|
| `surface` | page, panels, fields | white | zinc-900 |
| `surface-sunken` | hover of rows and outline buttons, gutter of the editors | zinc-100 | zinc-800 |
| `surface-raised` | active tab, grid header, selected source, highlighted menu item | zinc-100 | zinc-800 |
| `surface-strong` | strong hover | zinc-200 | zinc-700 |
| `fg` | regular text | zinc-700 | zinc-200 |
| `fg-secondary` | secondary text, labels of fields, inactive tabs, status bars | zinc-500 | zinc-400 |
| `fg-faint` | placeholders, chevrons, gutters, `NULL`, ghost buttons at rest | zinc-400 | zinc-500 |
| `fg-strong` | titles, values, active tab, text of fields | zinc-900 | white |
| `line` | borders | zinc-200 | zinc-700 |
| `line-faint` | inner lines of the grid and of the console | zinc-100 | zinc-800 |
| `primary` | action, selection, links, pressed toggles | blue-500 | blue-400 |
| `primary-hover` | hover of `primary` | blue-600 | blue-300 |
| `on-primary` | text on `primary` and `error` | white | zinc-900 |
| `success` | success | green-600 | green-400 |
| `warning` | warning | amber-600 | amber-400 |
| `info` | state of a session (open transaction), told without alarm | indigo-600 | indigo-300 |
| `error` | error, destructive action | red-600 | red-400 |
| `error-hover` | hover of `error` | red-700 | red-300 |
| `overlay` | veil behind the windows | zinc-900 at 40% | black at 60% |

Other variables of the theme:

- Selection of the grid: `primary` at 18%. Hover band of the resize handles: `primary` at 30%.
- Tooltips: background zinc-800 (light) / zinc-700 (dark), text zinc-100, in both modes.
- Flavor badges of the sources: MySQL, text blue-600 / blue-300 on the same color at 14%; MariaDB, text amber-700 / amber-300 on the same color at 16%.
- SQL highlighting (light / dark): keyword violet-600 / violet-300, type cyan-700 / cyan-300, string amber-700 / amber-300, number green-700 / green-300, variable pink-700 / pink-300, comment zinc-500 / zinc-400. JSON and XML add member and element names blue-600 / blue-300, attributes cyan-700 / cyan-300.
- Highlights of the code editors: cursor line `fg` at 4% and its gutter at 8%; selection `primary` at 25%, its other occurrences at 12%; matching bracket `success` at 25%, unmatched `error` at 25%; query that Ctrl+Enter would run `primary` at 7%; search results lemonchiffon (light), lemonchiffon at 12% (dark).
- Scrollbars: thin (2 steps), without arrows, transparent track, rounded thumb zinc-300 / zinc-600, hovered zinc-400 / zinc-500.

## 5. Layout

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ Prisme v0.7.0                                                (sync)  [Système ▾]  (SF) │
├─────────────────┬────────────────────────────────────────────────────────────┬─────────┤
│ Navigateur  [«] │ [boutique ×] [prod ×]  [+]                          [h][^] │ [»]     │
│ d’objets        │ [▶ Exécuter] [Exécuter précisément] [Exécuter tout]        │ Explo-  │
│                 │ [Enjoliver] [1 000 lignes ▾]         (Transaction ouverte) │ rateur  │
│ ▾ Sources de    │ ┌─ SQL editor ───────────────────────────────────────────┐ │ de      │
│   données       │ │ 1  SELECT c.id, c.name, c.email …                      │ │ données │
│   ▾ local       │ └────────────────────────────────────────────────────────┘ │ (rail,  │
│     ▸ shop      │ ═════════════════════════ handle ═════════════════════════ │ closed  │
│     ▸ crm       │ [● 1 · SELECT c.id… * ×] [● 2 · SELECT …]        [r][f][v] │ by      │
│   ▸ prod        │ ┌─ result grid ──────────────────────────────────────────┐ │ default)│
│ ▸ Notebooks     │ │    123 id   Aa name        {} attributes               │ │         │
│                 │ │  1   1042   Ada Martin     {"plan":"pro",…}            │ │         │
│                 │ └────────────────────────────────────────────────────────┘ │         │
│                 │ 4 lignes · exécution 12 ms · transfert 4 ms · …            │         │
│                 ├────────────────────────────────────────────────────────────┤         │
│                 │ [Historique] [Debug]                             [f][c][v] │         │
│                 │ #    Heure     Action          Message           Lignes …  │         │
└─────────────────┴────────────────────────────────────────────────────────────┴─────────┘
```

Legend: `[«]`, `[^]`, `[v]` fold buttons (`PanelLeftClose`, `PanelTopClose`, `PanelBottomClose`); `[»]` icon of the explorer's rail (`PanelRightOpen`); `[+]` new editor; `[h]` reopen a closed editor (`History`); `[r]` refresh (`RefreshCw`); `[f]` filters (`Funnel`); `[c]` clear the console (`MessageCircleOff`); `(sync)` synchronisation icon; `(SF)` avatar; `●` progress dot; `*` pin; `×` close. The buttons of the SQL cell stand on a single line; the diagram wraps them.

- **Top bar** across the whole width. Below it, three columns share the width in splitters: the object navigator on the left (default width 100 steps, smallest 67), the editors in the middle (smallest width 213 steps), the data explorer on the right (default width 140 steps, smallest 80; closed by default). Both side panels go from the top bar down to the bottom of the window.
- **Console** under the editors, between the side panels (default height 73 steps, smallest 40); the editors and the console share the height in a splitter.
- **Inside an editor**: the editor with its buttons on top, the results below, in a splitter of their own; the editor takes a third of the height by default. Either one folds: the folded editor disappears with its buttons, under the editors' tabs; the folded results keep their tab bar. Both are never folded at once.
- **Padding**: side panels `p-4`; the editors' area `py-4`, with `px-4` inside each part; the console `px-4 pt-1 pb-4`.
- **Splitter handle**: the line between two panels (1px border, `line`), grabbed over a band 3 steps wide, with a grip in its middle (9 steps long, 1 step thick, rounded, `line` color). On hover or drag, the band takes `primary` at 30% and the grip `primary`. Dragging moves the line within the bounds of the panels; dragged further, the line shrinks the next panel down to folding it. A double click folds or unfolds the panel it serves. The layout is remembered.
- **Rail of a folded panel**: a strip 9 steps thick along its edge, which is wholly a button: icon (`PanelLeftOpen`, `PanelRightOpen`, `PanelBottomOpen`) and title, `text-base` semibold, `fg-faint`, hovered `fg-strong` on `surface-sunken`. On a side, the icon at the top, at the height of the header's fold button, then the title written downwards; at the bottom, the title at the left end, the icon at the right end.
- **Panel header**: title (`text-lg` semibold `fg-strong`) on a line of a control's height (`h-8`), an optional line under it (`text-sm fg-secondary`), the actions (small ghost buttons) on the right, and the fold button (`PanelLeftClose`, `PanelRightClose`, `PanelBottomClose`). The fold button stands where the rail shows its icon: at the right end of the header for the navigator and the console, first, left of the title, for the data explorer.

## 6. Building blocks

- **Button**: three looks × three tones. Icon on the left of the label; without a label, the button is square (`w-8`, small `w-7`). `rounded-lg`, `font-medium`. Disabled: 50% opacity, `not-allowed` cursor. Loading: a spinning `LoaderCircle` replaces the icon, the button is inactive.
  - `solid`: neutral `bg-fg-strong text-surface`, primary `bg-primary text-on-primary`, error `bg-error text-on-primary`, each with its hover.
  - `outline` (the default): neutral `border-line bg-surface text-fg-strong`, hover `bg-surface-sunken`; primary `border-primary/50 text-primary`, hover `bg-primary/15`; error `border-error/50 text-error`, hover `bg-error/12`.
  - `ghost`: no background; neutral `text-fg-faint`, hover `text-fg-strong`; primary `text-primary`; error `text-error`. A pressed toggle (pin, filter, search, line wrap) takes the primary tone, a toggle whose panel is open adds `bg-surface-raised`.
- **Text field**: `h-8 rounded-lg border-line bg-surface px-3 text-base text-fg-strong`, placeholder `fg-faint`; optional icon on the left (`fg-faint`), optional small ghost button on the right (clear, show the password); focus ring. Multi-line field: the same, in `font-mono`, resizable vertically.
- **List of choices** (Reka `Select`): trigger shaped like a field, with a `ChevronDown` in `fg-faint`, hover `bg-surface-sunken`, optional icon of the current choice. Its list: `rounded-xl border-line bg-surface p-1 shadow-lg`, one step below its trigger; items `h-8 rounded-lg px-3 text-fg`, highlighted `bg-surface-raised text-fg-strong`, the current one checked with a `Check` in `primary`.
- **Menu** (Reka `DropdownMenu`, "⋮" button `EllipsisVertical`): the same box as the list, at least 53 steps wide, aligned with the end of its trigger; each item an icon and a label; a destructive item in `error`; an inactive item at 50%.
- **Window** (Reka `Dialog`): centered, at most 149 steps wide, `rounded-xl border-line bg-surface p-4 gap-4 shadow-xl`, on the `overlay` veil. Header: title `text-lg` semibold `fg-strong`, description `fg-secondary`, small ghost close button (`X`). Footer: buttons `gap-3`; the main button fills the rest of the width (`flex-1`), or the buttons are aligned at the end for a confirmation.
- **Form field**: label in a column 37 steps wide (`fg-secondary`), control over the remaining width, `gap-3`; for a multi-line control, the label aligns with its top.
- **Banner** (`UiAlert`): `rounded-lg px-3 py-3`, icon, message (line breaks kept), optional actions, then a small ghost close button in the current color. Tones: info `bg-primary/15 text-primary` (`Info`), success `bg-success/12 text-success` (`CircleCheckBig`), warning `bg-warning/12 text-warning` (`CircleAlert`), error `bg-error/12 text-error` (`CircleX`).
- **Toast**: bottom right, stacked, 96 steps wide, `rounded-xl border-line bg-surface px-4 py-3 shadow-lg`, success or error icon, text `fg-strong`; it fades after a few seconds, paused on hover.
- **Tooltip**: a single bubble for the page, after a short hover or on keyboard focus: `text-sm`, `px-3 py-1`, `rounded-lg`, dark in both modes, at most 96 steps wide and 12 lines; above its element, centered, below when there is no room; no arrow; the pointer passes through it. Almost every element has one: it explains the action and its shortcut.
- **Status bar**: a line of `text-sm fg-secondary` items, separated by a middle dot `·` with one step of room on each side; items wrap whole.
- **Document tab** (editors' bar and results' bar, same look): `rounded-t-lg py-1 ps-3 pe-1 gap-3`, `text-base` medium `fg-secondary`, hover `fg-strong`; active: `bg-surface-raised text-fg-strong`. At most 107 steps wide, label truncated. Its small ghost buttons (pin, close `X`) show on hovering the tab or on the active tab. The bar draws the line under the tabs. A double click on the label renames it (small field `h-7`, 53 steps wide, `text-sm`, with the focus ring); dragging a tab moves it, with a 2px `primary` insertion marker; a middle click closes it.
- **Small tabs** (console): `h-7 rounded-lg px-1 text-sm` medium `fg-secondary`, active `bg-surface-raised text-fg-strong`.
- **Toggle group** (console filters): small buttons `h-7 rounded-lg border-line px-1 text-sm` medium `fg-secondary`, hover `bg-surface-sunken`; pressed: `border-primary/50 bg-primary/15 text-primary`. Several can be pressed; none pressed shows everything.
- **Progress dot** of a query, 3 steps wide: waiting, `fg-faint` pulsing very slowly (3 s); running, `primary` pulsing (1.4 s); receiving rows, `primary` pulsing fast (0.6 s); done, `success`; failed, `error`. The pulse goes from full opacity and size to 35% and 70%.
- **Progress bar** on the bottom edge of a result tab, 2px: `primary` while rows arrive, its width following the share received, or a segment of 30% going back and forth when the share is unknown; `success` at the end, `error` on failure.
- **Check box**: native, `size-5`, `accent-primary`.

## 7. Areas

### 7.1 Top bar

`border-b border-line px-4 py-1`, items `gap-3`.

- Left: **Prisme** (`text-2xl` semibold `fg-strong`), then the version `v0.7.0` (`text-xs fg-faint`), on the same baseline.
- Right:
  - synchronisation icon, `size-5`, `fg-secondary`, explained by its tooltip: `Database` nothing exchanged yet, `DatabaseArrowDown` reading, `DatabaseArrowUp` saving, `DatabaseCheck` everything saved, `DatabaseX` in `error` when the last save was refused;
  - theme list, 37 steps wide: **Système** (System, `Monitor`), **Clair** (Light, `Sun`), **Sombre** (Dark, `Moon`);
  - round avatar (`size-8`, `rounded-full`, initials `text-sm` semibold, `text-primary` on `bg-primary/15`; hovered or open, `bg-primary text-on-primary`), which opens the account menu: **Profil** (Profile, `User`), **Paramètres** (Settings, `Settings`).

### 7.2 Object navigator (left panel)

Header **Navigateur d’objets** (Object navigator), then a tree with virtual scrolling, `gap-3`.

- Rows all of the same height (`--row-height`, 8 steps), `rounded-lg`, `gap-1`, hover `bg-surface-sunken`; each level indented by 4 steps. A row: chevron (`ChevronRight` / `ChevronDown`, `size-4 fg-faint`; an empty box of the same size without children), icon of its level (`size-5 fg-secondary`), label truncated with "…", optional note (`text-sm fg-faint`: `UNIQUE`, `désactivé` (disabled), `chargement…` (loading)), then the buttons that show on hover.
- Structure and icons:
  - **Sources de données** (Data sources, `Server`, bold): one branch per saved connection;
    - source (`Cable`), with its flavor badge right after the name (`mysql`, `mariadb`: `rounded-full px-1 text-xs` medium, in the engine's colors; tooltip: name and version of the server); the source of the active editor is highlighted (`bg-surface-raised`);
      - database (`Database`); the current database of the active editor in bold;
        - folders (`Folder` / `FolderOpen`) with their count: **Tables (n)**, **Vues (n)** (Views), **Procédures (n)**, **Fonctions (n)** (Functions), **Événements (n)** (Events);
        - table (`Table`), view (`Eye`), procedure (`ScrollText`), function (`SquareFunction`), event (`CalendarClock`);
          - folders of a table: **Colonnes (n)** (Columns), **Index (n)**, **Clefs étrangères (n)** (Foreign keys), **Déclencheurs (n)** (Triggers);
          - column on three aligned parts: icon (`KeyRound` primary key, `Link` foreign key, `Columns3` otherwise), name (bold if indexed, the names of a table aligned on the longest), details `text-sm fg-faint`: type in capitals, `NULL` when nullable, `AUTO_INCREMENT` (`INT UNSIGNED AUTO_INCREMENT`, `VARCHAR(200) NULL`);
          - index (`ListOrdered`, label `name (columns)`, note `PRIMARY KEY` or `UNIQUE`), foreign key (`Link`), trigger (`Zap`);
  - **Notebooks** (`Notebook`, bold): empty for now; its menu offers **Créer un notebook** (Create a notebook, `NotebookPen`), inactive.
- While loading: a spinning `LoaderCircle` row **chargement…**.
- Buttons on hover, at the right of the row (small ghost):
  - database: `PlugZap`, **Utiliser cette base (USE)** (Use this database); on the current database, `Unplug`, which closes the session;
  - table or view: `Rows3`, **Afficher le contenu (SELECT)** (Show the content);
  - index: `PencilRuler`, writes the operations on the index in the editor, commented out;
  - source: `Unplug` **Déconnecter** (Disconnect), enabled on the active editor's source once its session is open;
  - "⋮" menu: on the root, **Ajouter une connexion** (Add a connection, `Plus`), **Tout rafraîchir** (Refresh all, `RefreshCw`); on a source, **Modifier** (Edit, `Pencil`), **Rafraîchir** (Refresh), **Tester** (Test), **Déconnecter** (Disconnect, `Unplug`), **Supprimer** (Delete, `Trash2`, in `error`).
- A click opens or closes a row; Ctrl+click or Enter acts on it, like its button (choose the source, `USE` the database, show the table).
- Sticky rows: while the tree scrolls, the source, the database and the table of the rows on screen stay pinned at the top, over a `border-b border-line` on `bg-surface`.
- Banners above the tree: result of a connection test, reading errors of a source (warning, prefixed with its name).

### 7.3 Editors' bar

Document tabs, one per SQL editor; label: the name given by the user, else the name of the connection. After the tabs: `+` (`Plus`, **Nouvel éditeur**, New editor). At the right end: `History` (**Rouvrir un éditeur fermé**, Reopen a closed editor: a menu of the editors closed less than 24 hours ago), then the fold button of the active editor (`PanelTopClose` / `PanelTopOpen`).

An editor without a connection shows, centered: a round `bg-surface-raised p-4` disc with a `Database` icon in `fg-secondary`, and the text "Faites Ctrl+clic (Cmd+clic sous macOS) sur une source de données pour la choisir pour cet éditeur, ou ajoutez une connexion par le menu « ⋮ »" (Ctrl+click a data source to choose it for this editor, or add a connection from the "⋮" menu), `fg-secondary`, balanced, at most `max-w-xs`.

### 7.4 SQL cell: buttons and editor

- Buttons row, `gap-3`, regular size:
  - **Exécuter** (Run, `Play`, solid primary): the query under the cursor (Ctrl+Enter);
  - **Exécuter précisément** (Run exactly, `ScanText`, outline): only the selected text, inactive without a selection;
  - **Exécuter tout** (Run all, `FastForward`, outline): every query, or those the selection touches (Ctrl+Shift+Enter);
  - while queries run, the button that started them becomes a stop button (`Square`, error tone) and the other two wait, inactive;
  - **Enjoliver** (Beautify, `WandSparkles`, outline): rewrites the SQL in a consistent style, active once the catalog is loaded;
  - row limit list, 43 steps wide: **1 000 lignes**, **2 000 lignes**, **5 000 lignes**, **10 000 lignes**, **50 000 lignes**;
  - at the end (`ms-auto`), while a transaction is open: badge **Transaction ouverte** (Transaction open), `rounded-full px-4`, `text-base` medium, `text-info` on `bg-info/14`, as tall as the buttons.
- SQL editor (CodeMirror 6): `rounded-lg border-line`, `font-mono`, text `text-lg`, takes the remaining height. Gutter of line numbers only (`bg-surface-sunken`, `fg-faint`, `text-sm`, right-aligned, room for 4 digits, a `line` border on its right). Highlights of section 4: query that Ctrl+Enter would run, cursor line, selection, brackets. Syntax errors underlined in red, with their message on hover. Completion list of names (databases, tables, columns, functions).

### 7.5 Results

- Banners first, when they apply: **Session fermée.** (Session closed.) in warning, with a **Reconnecter** (Reconnect) outline button; server older than the oldest supported version (warning); error of the session (error).
- While the saved results load: a centered spinning `LoaderCircle` and **Chargement des résultats…** (Loading the results…), `fg-secondary`.
- Results' bar: one document tab per query, with its progress dot, its label (`1 · SELECT c.id, c.name, c.email FROM customer c…`: number, then the start of the query, 40 characters), its pin (`Pin`, always visible and in primary when pinned) and its close button; its progress bar on the bottom edge. At the right end: **Rafraîchir** (Refresh, `RefreshCw`; while it runs, `RefreshCwOff` in primary, which stops it), the filter toggle (`Funnel`, in primary while a filter text applies), then the fold button of the results (`PanelBottomClose` / `PanelBottomOpen`).
- Content of the tab shown, `gap-3`: error banner of the query; the filter's form when its toggle is pressed (field with a `Search` icon, placeholder **Filtrer** (Filter), at most 80 steps wide, clear button; list **Toutes les colonnes** (All columns) or one column, at most 67 steps wide); the grid; the status bar.
- Status bar: **120 / 5 000 lignes** (rows kept by the filter / rows received; **5 000 lignes** without a filter) · **exécution 12 ms** · **transfert 48 ms** · **1,2 Mo** · **25,0 Mo/s** · **28/10/2026 14:32:05** (end of the execution: the instant the result reflects). During the query, the counts change once per second.

### 7.6 Result grid

Own component with virtual scrolling (tens of thousands of rows). Frame: 1px `line` border; its top border is the line under the results' bar, which the grid touches.

- Rows of a fixed height (`--grid-row-height`, 8 steps), text `text-base`, centered on its capitals.
- Header, sticky at the top, `bg-surface-raised`, semibold: icon of the type's family (`size-4 fg-faint`), the column's name, then the sort button (`⇅`, `▲`, `▼`, `text-sm fg-faint`, hover `fg-strong`). The native type (`VARCHAR(20)`, `DECIMAL(10,2)`) shows in the tooltip. Type marks: integer `123` and decimal `.00` written in `font-mono text-sm` medium, float `Hash`, text `CaseSensitive`, binary `Binary`, date `Calendar`, time `Clock`, date and time `CalendarClock`, year `CalendarDays`, JSON `Braces`, XML `CodeXml`, web address `Link`, bit `ToggleRight`, enum `List`, set `ListChecks`, spatial `Shapes`, other `CircleDashed`.
- Row numbers, sticky on the left with the top left corner (which selects everything): `bg-surface-raised`, `font-mono text-sm fg-faint`, right-aligned, as wide as the largest number (4 digits at least, like the editor's gutter), a `line` border on their right.
- Cells: `px-1`, `line-faint` borders between cells and rows, text truncated with "…". Numbers, dates and times right-aligned; `CHAR` columns in `font-mono`; `NULL` in italic small capitals, `fg-faint`; web addresses as `primary` links with an `ExternalLink` icon, underlined on hover.
- Selection: cells, rows, columns or ranges, with `primary` at 18%. Copy (Ctrl+C) gives tab-separated text.
- Column edges: dragging the right edge of a header resizes the column (hover band `primary` at 30%), a double click fits it to its content; dragging a name moves the column, with a 2px `primary` insertion marker.

### 7.7 Data explorer (right panel, closed by default)

Shows the JSON or XML values selected in a column of the grid, in a read-only code editor.

- Header **Explorateur de données** (Data explorer), its fold button first (`PanelRightClose`), and under the title the subtitle `attributes · JSON · 3 valeurs` (column · format · count), `text-sm fg-secondary`.
- Actions (small ghost buttons): **Tout replier** (Fold all, `ChevronsDownUp`), **Tout déplier** (Unfold all, `ChevronsUpDown`), **Replier le niveau du curseur** (Fold the cursor's level, `FoldVertical`), **Déplier le niveau du curseur** (`UnfoldVertical`), **Rechercher** (Search, `Search`, toggle), **Retour à la ligne** (Line wrap, `TextWrap`, toggle), **Copier le document** (Copy the document, `Copy`).
- Search bar, when open: a field (placeholder of a JSONPath expression for JSON, XPath for XML; red border while the expression is invalid), the counter **2 / 7** (`text-sm fg-secondary`), previous and next result (`ChevronUp`, `ChevronDown`), filter mode (`Funnel`, toggle: shows only the results); the error of the expression under it, `text-sm text-error`.
- Document: `rounded-lg border-line`, `font-mono`, `text-lg`, indented by 4 spaces, highlighted, fold arrows in the gutter; several values are gathered into a `[…]` array (JSON) or a `<data>…</data>` element (XML).
- Without a document, a message in `fg-secondary`: "Sélectionnez des valeurs JSON ou XML d’une colonne de la grille." (Select JSON or XML values of a column of the grid.), or "La sélection couvre plusieurs colonnes : sélectionnez des valeurs d’une seule colonne." (The selection covers several columns: select values of a single column.)

### 7.8 Console (bottom panel)

- Header: the small tabs **Historique** (History) and **Debug** in place of the title; actions: filters toggle (`Funnel`), clear the list shown (`MessageCircleOff`: **Effacer l’historique**, Clear the history, or **Effacer le debug**, Clear the debug log); fold button (`PanelBottomClose`) at the right end.
- Filters, when shown: **Action** (Session, Exécuter, Exécuter tout, Exécuter précisément, Rafraîchir, Base courante, Interruption, Navigateur, Test de connexion) for the history; **Niveau** (Level: Erreur, Avertissement, Info, Debug, Trace) and **Source** (Serveur, Interface) for the debug log; label `text-sm fg-secondary`, then a toggle group.
- Tables: `text-sm`, tabular figures, fixed layout, rows at their natural height (`py-1 px-3`), `line-faint` lines between rows, hover `bg-surface-sunken`; sticky header on `bg-surface`, `fg-secondary`, medium, with a `line` border under it. Numbers and durations right-aligned; long texts truncated, full text in the tooltip. Oldest rows at the top; the list stays scrolled to the bottom while new rows arrive.
  - History: **#** (`fg-faint`), **Heure** (Time, `14:32:05`), **Action**, **Message** (the SQL on one line, `font-mono fg-strong`, then its error in `error`), **Lignes** (Rows: `+1 000` when truncated, `—` in `fg-faint` without rows), **Exécution** (Execution), **Transmission**.
  - Debug: **#**, **Heure**, **Niveau** (Erreur in `error`, Avertissement in `warning`, Info in `fg`, Debug in `fg-secondary`, Trace in `fg-faint`), **Source**, **Message** (`font-mono fg-strong`).
- Empty list: centered `fg-secondary` text, **Aucune requête envoyée pour le moment** (No query sent yet), or **Aucune requête ne correspond aux filtres** (No query matches the filters).

### 7.9 Windows

- **Connection** (**Nouvelle connexion** / **Modifier la connexion**, New / Edit connection): fields **Nom** (Name, placeholder `prod, local...`), **Hôte** (Host), **Port**, **Compte** (Account), **Mot de passe** (Password, with a show / hide button `Eye` / `EyeOff`; `inchangé` (unchanged) as placeholder when editing), **Base** (Database, `facultatif`, optional), **À la connexion** (On connect: multi-line SQL field, `SET NAMES utf8mb4;` as placeholder), **Session gardée** (Session kept: a number field 27 steps wide, then "min après la fermeture de la page", minutes after the page closes), **Navigateur** (check box **Afficher les bases système**, Show the system databases). Banners: server error, test result (success with the flavor and version of the server, or warning), open transactions that saving would roll back. Footer: **Tester** (Test, outline, loading while it runs), then **Enregistrer** (Save, solid primary, over the rest of the width; **Enregistrer et annuler**, Save and roll back, in error tone when transactions are open).
- **Paramètres** (Settings), description "Langue de l’interface, requêtes, éditeur et conservation de la console": **Langue** (Language: Français; English, Español, Deutsch, Italiano listed, inactive), **Lignes** (default row limit), **Requêtes** (check box **Sessions en lecture seule**, Read-only sessions), **Écritures** (Writes: check boxes in `font-mono`, grouped by line with the group's name in `fg-secondary`: **Données** INSERT UPDATE DELETE TRUNCATE; **Tables** CREATE ALTER RENAME DROP; **Tables temporaires** CREATE ALTER RENAME DROP; **Vues** CREATE DROP; **Fonctions** CREATE DROP; **Procédures** CREATE DROP; **Événements** CREATE ALTER DROP; the whole group at 50% and inactive while the sessions are read-only), **Indentation** (number, "espaces"), **Rétention historique** and **Rétention debug** (number, "jours", days). Footer: **Annuler** (Cancel), **Enregistrer** (Save).
- **Profil** (Profile): **Nom**, **Prénom** (Last name, First name), **E-mail** shown read-only. Footer: **Annuler**, **Enregistrer**.
- **Confirmations**: **Supprimer la connexion** (Delete the connection: explains that only the settings saved in Prisme go, the server is untouched; buttons **Annuler**, **Supprimer** solid error). Ending a session whose transaction is open: **Déconnecter la session**, **Fermer l’éditeur** or **Changer de source** (Disconnect the session, Close the editor, Change the source), description "Une transaction est ouverte sur la session." (A transaction is open on the session.), text explaining the `ROLLBACK`, buttons **Annuler** and **Déconnecter et annuler** / **Fermer et annuler** / **Changer et annuler** (solid error), aligned at the end.

## 8. States to draw

1. Main screen, light, at rest: navigator open on a MariaDB source, two editors, a result tab done, console on its history.
2. The same screen, dark.
3. Queries running after **Exécuter tout**: three result tabs, one done, one receiving rows (progress bar), one waiting; the **Exécuter tout** button turned into a stop button, the other two inactive.
4. Open transaction: the **Transaction ouverte** badge, and the confirmation window of **Déconnecter la session**.
5. Errors: a failed query (red dot and bar, error banner above the grid, history row with its error), a syntax error underlined in the editor with its tooltip.
6. Session closed: the **Session fermée.** banner with **Reconnecter**.
7. Data explorer open on three JSON values, search in filter mode with its counter.
8. Grid with the filter's form open on one column, a range of cells selected, a column being moved (insertion marker), a `NULL`, a link, a `CHAR` column.
9. Navigator scrolled in a table's columns, with the sticky source, database and table, a "⋮" menu open on a source.
10. Folded panels: navigator and console folded into their rails, editor folded under the editors' bar.
11. Empty states: editor without a connection, empty console, data explorer without a selection, first launch without any connection.
12. Windows: connection (editing, after a successful test), settings (read-only sessions on, writes greyed out).

## 9. Sample data

- Sources: **local** (MariaDB 11.4.4, badge `mariadb`) with the databases `shop` and `crm`; **prod** (MySQL 8.4.3, badge `mysql`).
- `shop`: tables `customer`, `order`, `order_item`, `product`; view `customer_total`; procedure `refresh_stats`; function `vat`; event `purge_carts`.
- `customer`, as its columns show in the navigator: `id` `INT UNSIGNED AUTO_INCREMENT` (primary key), `email` `VARCHAR(254)` (unique index `email (email)`), `name` `VARCHAR(100)`, `country` `CHAR(2) NULL`, `website` `VARCHAR(200) NULL`, `attributes` `JSON NULL`, `created_at` `DATETIME(6)`.
- Editor **boutique** (on **local**, current database `shop`), next to an editor **prod** (on **prod**):

```sql
SELECT c.id, c.name, c.email, c.country, c.website, c.attributes, c.created_at
FROM customer AS c
WHERE c.created_at >= '2026-01-01'
ORDER BY c.created_at DESC;

SELECT country, COUNT(*) AS customers
FROM customer
GROUP BY country;
```

- Rows of the first query:

| id | name | email | country | website | attributes | created_at |
|---|---|---|---|---|---|---|
| 1042 | Ada Martin | ada.martin@example.org | FR | https://ada.example.org | `{"plan":"pro","tags":["beta","eu"]}` | 2026-10-27 18:04:12.000000 |
| 1041 | Léon Dubois | leon.dubois@example.org | BE | NULL | `{"plan":"free","tags":[]}` | 2026-10-26 09:15:40.000000 |
| 1040 | Inès Garcia | ines.garcia@example.org | ES | https://ines.example.org | NULL | 2026-10-25 21:47:03.000000 |
| 1039 | Noah Weber | noah.weber@example.org | NULL | NULL | `{"plan":"pro","tags":["eu"]}` | 2026-10-25 08:30:59.000000 |

- History of the console: a **Session** row (`SET NAMES utf8mb4`), two **Exécuter tout** rows (the two queries above, `+1 000` and `4` rows, durations in ms), a **Navigateur** row (`SELECT … FROM information_schema.TABLES …`), a failed **Exécuter** row (`SELECT * FROM custome` — `Table 'shop.custome' doesn't exist`).

## 10. Coming next (context only)

These features are planned. They are not drawn now, but the designs leave room for them:

- other engines: PostgreSQL, SQLite, DuckDB, each with its flavor badge and its color;
- translation between SQL dialects: the input dialect of an editor can differ from the dialect of its source, with a preview of the translated query that marks the emulated functions, and emulated or untranslatable functions underlined in the editor;
- notebooks (the **Notebooks** root of the navigator), several languages per notebook (Tutorial D, later PRQL and Kusto), one per query;
- metrics on databases, tables and columns (size, estimated row count, NULL rate);
- a graph of a database's tables and their foreign keys;
- copy and export of a result in several formats (tabular, CSV, TSV, JSON, HTML, XML, box-drawn table);
- a multilingual interface (English, Spanish, German, Italian).
