# Design

Material for Claude Design, which draws proposals of interface from a brief:

- `SPECIFICATIONS.md`: the brief of Prisme's interface, pasted whole into Claude Design with the
  screenshots.
- `screenshots/`: the screenshots the brief refers to, taken at a scale of 150 % on the test
  databases described in its section 9.
- `capture/`: what retakes them: the script `capture.mjs`, which drives a headless Chrome through
  the DevTools protocol (`devtools.mjs`).

The test databases are those of the latest MariaDB and MySQL LTS among the development servers
(`compose.yaml`, see `docs/DEVELOPER.md`, section "Development servers"), loaded by
`servers/mariadb.sql` and `servers/mysql.sql`.

## Retaking the screenshots

Prerequisites: Docker Desktop running, Node 22 or later, Google Chrome, and Prisme built with its
interface (`npm run build` in `web/`, then `cargo build`). Ports 8485 and 9333 must be free.

1. Start the servers of the test databases, from the root of the repository:

   ```
   docker compose up --detach --wait mariadb-lts mysql-lts
   ```

2. Still from the root of the repository:

   ```
   node design/capture/capture.mjs
   ```

   The script starts Prisme on `127.0.0.1:8485` with a throwaway data folder, and a headless Chrome
   with a throwaway profile. It prepares the connections and the editors through the API, runs the
   queries, then writes into `design/screenshots/`, each in light and dark (`-light.png`,
   `-dark.png`): the main screen (`main`), each menu open, cut around it and its trigger (`menu-*`),
   and the data explorer (`explorer`). At the end, even after a failure, it stops both programs and
   removes their folders. A folder given as argument receives the screenshots instead;
   `PRISME_BIN` and `CHROME_BIN` name other executables.

The data are the same from one run to the next; durations and times change.
