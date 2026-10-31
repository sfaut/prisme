# Dropped dependencies

This document keeps the dependencies that Prisme dropped, as a record of experience: what each one
served, why it left, what replaced it, what was lost with it, and what it taught. One entry per
dependency, or per group dropped together, the most recent first. A dependency that leaves the
manifests (`web/package.json`, a `Cargo.toml` of the workspace) gets its entry in the same change;
a dependency only removed from one crate while another one keeps it does not.

Related records: the options set aside before any installation are in `docs/QUESTIONS.md`, section 3
"Ideas dropped or postponed"; the dependencies kept at an older version, in `docs/DEVELOPER.md`,
section "Held-back dependencies"; the workarounds of the dependencies still used, in the section
"Workarounds" of the same guide.

| Dependency | Part | Used | First version | Last version | First Prisme | Last Prisme | Replaced by | Reason |
|---|---|---|---|---|---|---|---|---|
| `@tanstack/vue-virtual` | interface | 2026-10-12 to 2026-10-31 | 3.13.43 | 3.13.49 | v0.1.0 | v0.8.0 | windowing of the result grid | cost of each update, quadratic over a reception |
| `testcontainers-modules` | tests of the server | 2026-10-12 to 2026-10-31 | 0.15.0 | 0.15.0 | v0.1.0 | v0.8.0 | the servers of `compose.yaml` | one set of servers for the tests, the screenshots and the measures, on every supported version |
| `lucide-vue-next` | interface | 2026-10-14 to 2026-10-21 | 1.0.0 | 1.0.0 | v0.1.0 | v0.4.0 | `@lucide/vue` | published under a new name |
| `@nuxt/ui`, `@iconify-json/lucide` | interface | 2026-10-13 to 2026-10-14 | 4.11.3, 1.2.143 | 4.11.3, 1.2.143 | — | — | Reka UI and components of Prisme | its theme layer |
| `naive-ui` | interface | 2026-10-12 to 2026-10-13 | 2.45.3 | 2.45.3 | — | — | Nuxt UI | rendering through classes, theme through tokens |

The first and last versions are those of the lock files (`web/package-lock.json`, `Cargo.lock`)
while the dependency was used. The first and last versions of Prisme are the published versions
(tags) that held it; a dash, when no published version held it.

An entry gives, as far as they apply:

- **Role**: what the dependency served, with the dates and the commits of its arrival and of its
  departure;
- **Why it left**: the reason, with its measures;
- **Replaced by**: what does the work now;
- **What was used of it**, and **what was not**, when code of Prisme takes over;
- **What is lost**, and **what stays** (a package still installed for another dependency, for
  example);
- **Upstream**: the ticket posted or drafted (`docs/upstream/`);
- **To reconsider**: the cases that would bring it back;
- **Lessons**: what the experience teaches for the next choices.

## `@tanstack/vue-virtual`

- **Role**: the virtual scrolling of the result grid (`ResultGrid.vue`): it drew only the visible
  rows, and gave the height of the whole result to the scroll bar. Added on 2026-10-12 (`324d335`),
  dropped on 2026-10-31 (`10d0fac`), at version 3.13.49 (`@tanstack/virtual-core` 3.19.8).
- **Why it left**: its cost per update follows the rows received so far. Each time the number of
  rows changes, `getMeasurements` computes again the position of every row, from the first one, and
  allocates arrays of the size of the list. A reception adds a batch every 100 ms: its total cost
  grows with the square of the rows. At 1 million rows, 7.6 s of the 32.3 s of main thread of the
  reception (24 %); at 200,000 rows, 0.40 s: 5 times more rows, 19 times more time
  (`docs/PERFORMANCES.md`, chapter 12). Same code in 3.19.9.
- **Replaced by**: the windowing of the grid (`drawnRange` in `web/src/components/resultGrid.ts`).
  Its rows have a fixed height: the first row drawn is the scroll position divided by the height of
  a row, the last one adds the height of the visible area, with 20 rows of margin on each side. A
  passive scroll listener and a `ResizeObserver` give these positions. The cost is two divisions,
  whatever the number of rows. With the other changes of chapter 12, 1 million rows are received in
  9.0 to 13.5 s, against 23.5 to 37.8 s before.
- **What was used of it**: the number of rows, the total height, the visible rows with a margin of
  20 (`overscan`), the observation of the scroll element.
- **What was not**: the measure of each row drawn (`measureElement`, for rows of variable heights),
  the scroll to a given row (`scrollToIndex`: the keys of the grid are Ctrl+C, Ctrl+A and Escape),
  the horizontal virtualization, the lanes (`lanes`, for masonry layouts).
- **What is lost**: rows of variable heights (a cell on several lines of text) would need a
  virtualizer again. The fixes and the features of the next versions of TanStack no longer reach
  the grid, which keeps about 40 lines of its own, tested in `resultGrid.spec.ts`.
- **What stays**: the package stays installed and in the bundle: Reka UI builds its virtualized
  tree on it (`TreeVirtualizer`, in the object navigator, whose count changes only when a node
  opens or closes). The interface has one direct dependency less, and the same bytes. The Pivoteur
  can rely on it for its columns (up to 5,000), whose number does not grow by batches.
- **Limit shared**: Chrome caps the height of an element at 33,554,428 pixels, with TanStack as
  without it. At 24 pixels per row, the grid scrolls down to row 1,398,101 at most.
- **Upstream**: ticket drafted in `docs/upstream/tanstack-virtual-count-growth.md`, for the
  maintainer to post. Its script, without a browser, grows a list by 5,000 items up to 1 million:
  1.7 to 3.5 s for the 200 updates. Close ticket:
  [TanStack/virtual#565](https://github.com/TanStack/virtual/issues/565).
- **To reconsider**: rows of variable heights; a version of TanStack whose update follows the rows
  added, which the script of the draft measures.
- **Lessons**: a library measured on lists of a fixed size can cost the whole list at each change of
  size, which a reception repeats hundreds of times. The cost per update is measured on the largest
  result, in a real browser, by a profile: jsdom draws no rows, and the tests did not see it. What a
  dependency brings is weighed against the part of it Prisme uses, here two divisions.

## `testcontainers-modules`

- **Role**: the tests on real servers (`crates/connectors`, `crates/server`) started a container of
  MySQL or MariaDB each. Added on 2026-10-12 (`778360c`), dropped on 2026-10-31 (`cf83347`).
- **Why it left**: the six servers of `compose.yaml` (for MySQL and MariaDB, the oldest version
  Prisme supports, the latest LTS and the latest version), started beforehand, serve the tests, the
  capture of the design screenshots and the performance measures alike.
- **Replaced by**: the tests connect to these servers. Each test works in a database of its own,
  created then dropped, and runs on every server of the engines it concerns; a server that does not
  answer fails its tests with the command that starts it. The CI starts the servers before the
  backend compiles.
- **What is lost**: a test no longer starts its server itself: `docker compose up --detach --wait`
  comes first.
- **Lessons**: one set of servers, described once, serves every use and covers each supported
  version.

## `lucide-vue-next`

- **Role**: the icons of the interface. Added on 2026-10-14 (`c2607f5`), dropped on 2026-10-21
  (`3614cf9`).
- **Why it left**: Lucide publishes its Vue icons under `@lucide/vue`; `lucide-vue-next` stopped at
  1.0.0, without the new icons (`database-check`...).
- **Replaced by**: `@lucide/vue` 1.61.0; only the imports changed.
- **Lessons**: a package whose versions stop can go on under another name, announced by its project.

## `@nuxt/ui` and `@iconify-json/lucide`

- **Role**: the components of the interface (Nuxt UI, built on Reka UI, with Tailwind CSS), and the
  set of Lucide icons it reads. Installed on 2026-10-13 for a trial beside Naive UI (`9a71dea`),
  adopted the same day (`876a8ff`), dropped on 2026-10-14 (`c2607f5`).
- **Why it left**: its theme layer (configuration of each component, class merging, imposed sizes
  to override) got in the way of using Tailwind directly.
- **Replaced by**: Reka UI for the behaviour (keyboard, focus, positioning, accessibility) and small
  components of Prisme for the rendering (`web/src/components/ui/`), in direct Tailwind classes; the
  icons, by `lucide-vue-next`.
- **Lessons**: a library that styles its components brings a theme layer of its own, to override; a
  library of behaviour without style, rendered with the classes of the theme, keeps the theme in a
  single place (rule 10 of `CLAUDE.md`).

## `naive-ui`

- **Role**: the layout of the screen and the components of the interface. Added on 2026-10-12
  (`cbe5d65`), dropped on 2026-10-13 (`876a8ff`).
- **Why it left**: the redesign of the interface wanted the rendering controlled through classes, a
  theme through tokens, and light / dark without settings per component; decided after a trial of
  Nuxt UI on the object navigator.
- **Replaced by**: Nuxt UI.
- **Lessons**: its layout component, `NLayout`, takes the height of its content by default, not that
  of the window: the result grid was left without a bound on its height and drew all its rows
  (`docs/PERFORMANCES.md`, chapter 1). The defaults of a layout component are checked with a large
  result, in a real browser.

