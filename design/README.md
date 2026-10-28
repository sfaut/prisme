# Design

Material for Claude Design, which draws proposals of interface from a brief:

- `SPECIFICATIONS.md`: the brief of Prisme's interface, pasted whole into Claude Design with the
  screenshots.
- `screenshots/`: the screenshots the brief refers to, taken at a scale of 150 % on the test
  databases described in its section 9.
- `capture/`: what retakes them: the test databases (`mariadb.sql`, `mysql.sql`) and the script
  `capture.mjs`, which drives a headless Chrome through the DevTools protocol (`devtools.mjs`).

## Retaking the screenshots

Prerequisites: Docker Desktop running, Node 22 or later, Google Chrome, and Prisme built with its
interface (`npm run build` in `web/`, then `cargo build`). Ports 3400, 3401, 8485 and 9333 must be
free.

1. Start the servers of the test databases. The first time:

   ```
   docker run --detach --name prisme-design-mariadb --publish 127.0.0.1:3400:3306 --env MARIADB_ROOT_PASSWORD=design mariadb:11.4.13
   docker run --detach --name prisme-design-mysql --publish 127.0.0.1:3401:3306 --env MYSQL_ROOT_PASSWORD=design mysql:8.4.11
   ```

   The next times: `docker start prisme-design-mariadb prisme-design-mysql`.

2. Load the databases, once the servers accept connections (the first time only):

   ```
   docker exec --interactive prisme-design-mariadb mariadb --user=root --password=design < design/capture/mariadb.sql
   docker exec --interactive prisme-design-mysql mysql --user=root --password=design < design/capture/mysql.sql
   ```

3. From the root of the repository:

   ```
   node design/capture/capture.mjs
   ```

   The script starts Prisme on `127.0.0.1:8485` with a throwaway data folder, and a headless Chrome
   with a throwaway profile. It prepares the connections and the editors through the API, runs the
   queries, then writes `main-light.png`, `main-dark.png`, `explorer-light.png` and
   `explorer-dark.png` into `design/screenshots/`. At the end, even after a failure, it stops both
   programs and removes their folders. A folder given as argument receives the screenshots instead;
   `PRISME_BIN` and `CHROME_BIN` name other executables.

The data are the same from one run to the next; durations and times change.
