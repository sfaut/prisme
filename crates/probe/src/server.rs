//! Database servers of the measurements: containers of the official Docker
//! images of MySQL and MariaDB, one per image, started on demand and left
//! running afterwards for the next measurements.
//!
//! The containers publish no port: the statements go through the client of
//! the image (`docker exec`), over TCP inside the container, so that a server
//! still being initialised is never reached. Their root password only
//! protects these throwaway servers.

use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::Duration;

use anyhow::{Context, bail};
use prisme_dialects::Dialect;
use prisme_dialects::versions::Version;

/// Root password of the containers of the measurements.
const PASSWORD: &str = "probe";

/// Database of the measurements, created in each server.
const DATABASE: &str = "probe";

/// Prefix of the names of the containers.
const PREFIX: &str = "prisme-probe";

/// Seconds allowed for a server to start.
const START_TIMEOUT: u64 = 300;

/// Warning of the clients about the password on the command line.
const PASSWORD_WARNING: &str =
    "mysql: [Warning] Using a password on the command line interface can be insecure.";

/// Server of a measurement, ready to run statements.
#[derive(Debug, Clone)]
pub struct Server {
    /// Docker image, `mysql:8.0.11`.
    pub image: String,
    /// Dialect of the server.
    pub dialect: Dialect,
    /// Name of the container.
    pub container: String,
    /// Version of the server, as `SELECT VERSION()` gives it.
    pub version: String,
}

impl Server {
    /// Server of the image `image` (`mysql:8.4`, `mariadb:10.6`): its
    /// container is started, or created, then awaited.
    pub fn start(image: &str) -> anyhow::Result<Self> {
        let (dialect, container) = container_of(image)?;
        let mut server = Self {
            image: image.to_string(),
            dialect,
            container,
            version: String::new(),
        };
        server.ensure_running()?;
        server.version = server.wait()?;
        Ok(server)
    }

    /// Name and version of the server, `MariaDB 10.6.28`.
    pub fn label(&self) -> String {
        match Version::parse(&self.version) {
            Some(version) => format!("{} {version}", self.dialect.label()),
            None => format!("{} {}", self.dialect.label(), self.version),
        }
    }

    /// Version of the server.
    pub fn parsed_version(&self) -> anyhow::Result<Version> {
        Version::parse(&self.version)
            .with_context(|| format!("unreadable version {:?}", self.version))
    }

    /// Runs `script` with the client of the image, in the database of the
    /// measurements, with the client options `options`.
    pub fn run(&self, script: &str, options: &[&str]) -> anyhow::Result<Output> {
        self.client(script, options, Some(DATABASE))
    }

    /// Runs `script` with the client of the image, in `database` when given.
    fn client(
        &self,
        script: &str,
        options: &[&str],
        database: Option<&str>,
    ) -> anyhow::Result<Output> {
        let client = match self.dialect {
            Dialect::MySql => "mysql",
            Dialect::MariaDb => "mariadb",
        };
        let mut child = Command::new("docker")
            .args(["exec", "--interactive", &self.container, client])
            .args(["--protocol=TCP", "--host=127.0.0.1", "--user=root"])
            .arg(format!("--password={PASSWORD}"))
            .arg("--default-character-set=utf8mb4")
            .args(options)
            .args(database)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("running docker")?;
        let mut stdin = child.stdin.take().context("input of docker")?;
        let script = script.to_string();
        // Written from a thread: a long output must not block the input.
        let writer = thread::spawn(move || stdin.write_all(script.as_bytes()));
        let mut output = child.wait_with_output().context("running docker")?;
        writer
            .join()
            .map_err(|_| anyhow::anyhow!("writing to docker"))?
            .context("writing to docker")?;
        let stderr = String::from_utf8_lossy(&output.stderr).replace(PASSWORD_WARNING, "");
        output.stderr = stderr.trim().as_bytes().to_vec();
        Ok(output)
    }

    /// Runs `script`, then gives its output; an error when the client fails.
    pub fn run_checked(&self, script: &str, options: &[&str]) -> anyhow::Result<String> {
        let output = self.run(script, options)?;
        if !output.status.success() {
            bail!(
                "{}: {}",
                self.container,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Starts the container, or creates it from its image.
    fn ensure_running(&self) -> anyhow::Result<()> {
        let state = Command::new("docker")
            .args(["inspect", "--format", "{{.State.Running}}", &self.container])
            .output()
            .context("running docker")?;
        let running = String::from_utf8_lossy(&state.stdout);
        let status = if !state.status.success() {
            eprintln!("{}: creating the container", self.container);
            let variable = match self.dialect {
                Dialect::MySql => "MYSQL_ROOT_PASSWORD",
                Dialect::MariaDb => "MARIADB_ROOT_PASSWORD",
            };
            Command::new("docker")
                .args(["run", "--detach", "--name", &self.container, "--env"])
                .arg(format!("{variable}={PASSWORD}"))
                .arg(&self.image)
                .status()
        } else if running.trim() != "true" {
            eprintln!("{}: starting the container", self.container);
            Command::new("docker")
                .args(["start", &self.container])
                .status()
        } else {
            return Ok(());
        };
        if !status.context("running docker")?.success() {
            bail!("{}: the container does not start", self.container);
        }
        Ok(())
    }

    /// Waits until the server answers over TCP, creates the database of the
    /// measurements, then gives the version of the server.
    fn wait(&self) -> anyhow::Result<String> {
        let script = format!(
            "CREATE DATABASE IF NOT EXISTS {DATABASE};
SELECT VERSION();
"
        );
        for _ in 0..START_TIMEOUT {
            let output = self.client(&script, &["--batch", "--skip-column-names"], None);
            if let Ok(output) = output
                && output.status.success()
            {
                return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
            }
            thread::sleep(Duration::from_secs(1));
        }
        bail!("{}: no answer after {START_TIMEOUT} s", self.container)
    }
}

/// Dialect and container of the image `image`.
fn container_of(image: &str) -> anyhow::Result<(Dialect, String)> {
    let Some((name, tag)) = image.split_once(':') else {
        bail!("image {image:?}: expected a name and a tag, such as mysql:8.4");
    };
    let dialect = match name {
        "mysql" => Dialect::MySql,
        "mariadb" => Dialect::MariaDb,
        _ => bail!("image {image:?}: only the images mysql and mariadb are known"),
    };
    let valid = !tag.is_empty()
        && tag
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character));
    if !valid {
        bail!("image {image:?}: invalid tag");
    }
    Ok((dialect, format!("{PREFIX}-{name}-{tag}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_image_has_its_container() {
        let (dialect, container) = container_of("mysql:8.0.11").unwrap();
        assert_eq!(dialect, Dialect::MySql);
        assert_eq!(container, "prisme-probe-mysql-8.0.11");
        let (dialect, container) = container_of("mariadb:11.8").unwrap();
        assert_eq!(dialect, Dialect::MariaDb);
        assert_eq!(container, "prisme-probe-mariadb-11.8");
        assert!(container_of("postgres:17").is_err());
        assert!(container_of("mysql").is_err());
        assert!(container_of("mysql:8;rm").is_err());
    }

    #[test]
    fn the_label_gives_the_version() {
        let server = Server {
            image: "mariadb:10.6".to_string(),
            dialect: Dialect::MariaDb,
            container: "prisme-probe-mariadb-10.6".to_string(),
            version: "10.6.28-MariaDB-ubu2204".to_string(),
        };
        assert_eq!(server.label(), "MariaDB 10.6.28");
    }
}
