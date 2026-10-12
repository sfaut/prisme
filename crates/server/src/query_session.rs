//! Session de requêtes sur une connexion enregistrée, par WebSocket.
//!
//! Une session garde une connexion ouverte au serveur cible pendant toute la
//! durée du WebSocket (les transactions en auront besoin). Elle exécute les
//! requêtes une par une et envoie les résultats en streaming, par lots de
//! lignes, sans jamais charger tout le résultat en mémoire.
//!
//! Protocole, en JSON texte :
//!
//! ```text
//! serveur → client  {"type": "ready", "server": {"flavor": "mariadb", "version": "11.3.2-MariaDB"}}
//! client → serveur  {"sql": "SELECT ...", "row_limit": 2000}     row_limit facultatif
//! serveur → client  {"type": "columns", "columns": ["a", "b"], "sql": "SELECT ... LIMIT 2001", "limit": "added"}
//! serveur → client  {"type": "rows", "rows": [["1", null], ...]}  autant de lots que nécessaire
//! serveur → client  {"type": "done", "row_count": 2000, "truncated": true, "affected_rows": 0, "elapsed_ms": 42}
//! serveur → client  {"type": "error", "message": "..."}           à la place de la suite, en cas d'échec
//! ```
//!
//! La limite de lignes suit `docs/V1.md`, section 2.3 : `row_limit` de la
//! requête, sinon le réglage de l'utilisateur.

use std::time::Instant;

use axum::extract::ws::{Message, WebSocket};
use prisme_connectors::mysql::{self, Cell, Connection, Flavor, MysqlTarget, ServerInfo};
use prisme_dialects::Dialect;
use prisme_dialects::limit::{LimitOutcome, apply_default_limit};
use serde::{Deserialize, Serialize};

use crate::api::{AppState, CONNECT_TIMEOUT};
use crate::settings::{self, check_row_limit};

/// Nombre de lignes par message `rows`.
const ROW_BATCH_SIZE: usize = 500;

/// Requête envoyée par le client.
#[derive(Debug, Deserialize)]
struct QueryRequest {
    /// Texte SQL, une seule requête.
    sql: String,
    /// Limite de lignes pour cette requête seule, parmi les choix permis.
    row_limit: Option<u32>,
}

/// Traitement de la limite, tel qu'annoncé au client.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum LimitReport {
    /// Prisme a ajouté `LIMIT N + 1`.
    Added,
    /// La limite écrite par l'utilisateur s'applique.
    UserLimit,
    /// Requête sans lignes à limiter.
    NotApplicable,
    /// Requête non analysée : la lecture s'arrête d'elle-même après N lignes.
    Unparsed,
}

/// Message envoyé au client.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMessage<'a> {
    /// La connexion au serveur cible est ouverte.
    Ready { server: &'a ServerInfo },
    /// Début d'un résultat : colonnes, texte réellement exécuté, traitement de la limite.
    Columns {
        columns: Vec<String>,
        sql: &'a str,
        limit: LimitReport,
    },
    /// Lot de lignes.
    Rows { rows: &'a [Vec<Cell>] },
    /// Fin du résultat.
    Done {
        row_count: usize,
        truncated: bool,
        affected_rows: u64,
        elapsed_ms: u128,
    },
    /// Échec de la requête ou de la session.
    Error { message: String },
}

/// Le client a fermé le WebSocket : la session s'arrête.
struct SocketClosed;

/// Déroule une session : connexion au serveur cible, puis requêtes jusqu'à la fermeture du WebSocket.
pub async fn run(mut socket: WebSocket, state: AppState, user_id: i64, target: MysqlTarget) {
    let mut connection = match tokio::time::timeout(CONNECT_TIMEOUT, mysql::connect(&target)).await
    {
        Ok(Ok(connection)) => connection,
        Ok(Err(error)) => {
            let _ = send_error(&mut socket, error.to_string()).await;
            return;
        }
        Err(_) => {
            let message = format!(
                "aucune réponse du serveur en {} s",
                CONNECT_TIMEOUT.as_secs()
            );
            let _ = send_error(&mut socket, message).await;
            return;
        }
    };

    let info = match mysql::server_info(&mut connection).await {
        Ok(info) => info,
        Err(error) => {
            let _ = send_error(&mut socket, error.to_string()).await;
            return;
        }
    };
    let dialect = match info.flavor {
        Flavor::MySql => Dialect::MySql,
        Flavor::MariaDb => Dialect::MariaDb,
    };

    if send(&mut socket, &ServerMessage::Ready { server: &info })
        .await
        .is_ok()
    {
        while let Some(Ok(message)) = socket.recv().await {
            let Message::Text(text) = message else {
                // Ping, pong et binaire sont ignorés ; Close termine la boucle au tour suivant.
                continue;
            };
            let session = Session {
                socket: &mut socket,
                connection: &mut connection,
                target: &target,
                state: &state,
                user_id,
                dialect,
            };
            if session.handle(text.as_str()).await.is_err() {
                break;
            }
        }
    }

    let _ = connection.disconnect().await;
}

/// Ce dont une requête a besoin pendant son exécution.
struct Session<'a> {
    /// WebSocket du client.
    socket: &'a mut WebSocket,
    /// Connexion ouverte au serveur cible.
    connection: &'a mut Connection,
    /// Paramètres de connexion, pour interrompre une requête par une seconde connexion.
    target: &'a MysqlTarget,
    /// État de l'application, pour lire les réglages.
    state: &'a AppState,
    /// Utilisateur de la session.
    user_id: i64,
    /// Dialecte du serveur cible.
    dialect: Dialect,
}

impl Session<'_> {
    /// Traite un message du client : une requête à exécuter.
    async fn handle(self, text: &str) -> Result<(), SocketClosed> {
        let request: QueryRequest = match serde_json::from_str(text) {
            Ok(request) => request,
            Err(error) => {
                return send_error(self.socket, format!("message invalide : {error}")).await;
            }
        };
        let max_rows = match row_limit(self.state, self.user_id, request.row_limit).await {
            Ok(max_rows) => max_rows,
            Err(message) => return send_error(self.socket, message).await,
        };
        self.execute(&request.sql, max_rows).await
    }

    /// Exécute la requête et envoie son résultat en streaming.
    async fn execute(self, sql: &str, max_rows: u32) -> Result<(), SocketClosed> {
        let max_rows = max_rows as usize;
        let limited = match apply_default_limit(sql, self.dialect, max_rows as u64) {
            Ok(limited) => limited,
            Err(error) => return send_error(self.socket, error.to_string()).await,
        };
        // La lecture ne s'arrête d'elle-même que si Prisme a fixé la limite.
        let capped = matches!(
            limited.outcome,
            LimitOutcome::Added | LimitOutcome::Unparsed
        );

        let started = Instant::now();
        let connection_id = self.connection.id();
        let mut stream = match mysql::execute(self.connection, &limited.sql).await {
            Ok(stream) => stream,
            Err(error) => return send_error(self.socket, error.to_string()).await,
        };

        let columns = ServerMessage::Columns {
            columns: stream.column_names(),
            sql: &limited.sql,
            limit: report(limited.outcome),
        };
        send(self.socket, &columns).await?;

        let mut batch = Vec::with_capacity(ROW_BATCH_SIZE);
        let mut row_count = 0;
        let mut truncated = false;
        loop {
            let row = match stream.next_row().await {
                Ok(Some(row)) => row,
                Ok(None) => break,
                Err(error) => {
                    stream.discard().await;
                    return send_error(self.socket, error.to_string()).await;
                }
            };
            if capped && row_count == max_rows {
                // La ligne N + 1 : le résultat est tronqué, inutile de lire plus loin.
                truncated = true;
                if limited.outcome == LimitOutcome::Unparsed {
                    // Sans LIMIT, le serveur continuerait d'envoyer des lignes : on l'interrompt.
                    if let Err(error) = mysql::kill_query(self.target, connection_id).await {
                        tracing::warn!("interruption de la requête impossible : {error}");
                    }
                }
                break;
            }
            batch.push(row);
            row_count += 1;
            if batch.len() == ROW_BATCH_SIZE {
                send(self.socket, &ServerMessage::Rows { rows: &batch }).await?;
                batch.clear();
            }
        }
        if !batch.is_empty() {
            send(self.socket, &ServerMessage::Rows { rows: &batch }).await?;
        }

        let affected_rows = stream.affected_rows();
        stream.discard().await;
        let done = ServerMessage::Done {
            row_count,
            truncated,
            affected_rows,
            elapsed_ms: started.elapsed().as_millis(),
        };
        send(self.socket, &done).await
    }
}

/// Limite de lignes d'une requête : celle demandée, sinon le réglage de l'utilisateur.
async fn row_limit(state: &AppState, user_id: i64, requested: Option<u32>) -> Result<u32, String> {
    if let Some(row_limit) = requested {
        return check_row_limit(row_limit).map_err(|error| error.to_string());
    }
    state
        .with_db(move |db, _key| settings::load(db, user_id))
        .await
        .map(|settings| settings.row_limit)
        .map_err(|_| "réglages de l'utilisateur illisibles, voir le journal du serveur".to_string())
}

/// Traduit le traitement de la limite pour le client.
fn report(outcome: LimitOutcome) -> LimitReport {
    match outcome {
        LimitOutcome::Added => LimitReport::Added,
        LimitOutcome::UserLimit => LimitReport::UserLimit,
        LimitOutcome::NotApplicable => LimitReport::NotApplicable,
        LimitOutcome::Unparsed => LimitReport::Unparsed,
    }
}

/// Envoie un message au client.
async fn send(socket: &mut WebSocket, message: &ServerMessage<'_>) -> Result<(), SocketClosed> {
    let json = serde_json::to_string(message).expect("message sérialisable");
    socket
        .send(Message::Text(json.into()))
        .await
        .map_err(|_| SocketClosed)
}

/// Envoie un message d'erreur au client.
async fn send_error(socket: &mut WebSocket, message: String) -> Result<(), SocketClosed> {
    send(socket, &ServerMessage::Error { message }).await
}

#[cfg(test)]
mod tests {
    use futures_util::{SinkExt, StreamExt};
    use secrecy::SecretString;
    use serde_json::{Value, json};
    use testcontainers_modules::mariadb::Mariadb;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    use tokio::net::TcpStream;
    use tokio_tungstenite::tungstenite::Message as ClientMessage;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

    use crate::api::{AppState, router};
    use crate::connections::{self, NewConnection};
    use crate::master_key::MasterKey;
    use crate::store;

    type ClientSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

    /// Démarre Prisme sur un port libre, avec une connexion enregistrée vers
    /// `host:port` (compte root sans mot de passe, base `test`), et renvoie
    /// l'adresse du serveur et l'identifiant de la connexion.
    async fn start(host: &str, port: u16) -> (String, i64) {
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let new = NewConnection {
            name: "test".to_string(),
            host: host.to_string(),
            port,
            username: "root".to_string(),
            password: SecretString::from(""),
            database_name: "test".to_string(),
        };
        let id = connections::insert(&db, &key, user_id, &new).unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let app = router(AppState::new(db, key, user_id));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (address, id)
    }

    /// Lit le prochain message texte du serveur, en JSON.
    async fn next_json(socket: &mut ClientSocket) -> Value {
        loop {
            if let ClientMessage::Text(text) = socket.next().await.unwrap().unwrap() {
                return serde_json::from_str(&text).unwrap();
            }
        }
    }

    /// Envoie une requête, puis lit les messages jusqu'à `done` ou `error`.
    /// Renvoie le message `columns` (ou Null), le nombre de lignes reçues et le dernier message.
    async fn query(socket: &mut ClientSocket, request: Value) -> (Value, usize, Value) {
        socket
            .send(ClientMessage::Text(request.to_string().into()))
            .await
            .unwrap();
        let mut columns = Value::Null;
        let mut received = 0;
        loop {
            let message = next_json(socket).await;
            match message["type"].as_str().unwrap() {
                "columns" => columns = message,
                "rows" => received += message["rows"].as_array().unwrap().len(),
                _ => return (columns, received, message),
            }
        }
    }

    #[tokio::test]
    async fn unreachable_server_sends_an_error() {
        // Port 1 : aucun serveur n'y écoute.
        let (address, id) = start("127.0.0.1", 1).await;
        let (mut socket, _) = connect_async(format!("ws://{address}/api/connections/{id}/query"))
            .await
            .unwrap();

        assert_eq!(next_json(&mut socket).await["type"], "error");
    }

    #[tokio::test]
    async fn unknown_connection_is_refused() {
        let (address, _) = start("127.0.0.1", 1).await;
        assert!(
            connect_async(format!("ws://{address}/api/connections/999/query"))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "nécessite Docker Desktop démarré"]
    async fn streams_results_with_the_default_limit() {
        let server = Mariadb::default().start().await.unwrap();
        let host = server.get_host().await.unwrap().to_string();
        let port = server.get_host_port_ipv4(3306).await.unwrap();
        let (address, id) = start(&host, port).await;
        let (mut socket, _) = connect_async(format!("ws://{address}/api/connections/{id}/query"))
            .await
            .unwrap();

        let ready = next_json(&mut socket).await;
        assert_eq!(ready["type"], "ready");
        assert_eq!(ready["server"]["flavor"], "mariadb");

        // Pas de limite écrite : 1 000 lignes affichées sur 2 500, résultat tronqué.
        let (columns, received, done) = query(
            &mut socket,
            json!({ "sql": "SELECT seq FROM seq_1_to_2500", "row_limit": 1000 }),
        )
        .await;
        assert_eq!(columns["limit"], "added");
        assert_eq!(columns["sql"], "SELECT seq FROM seq_1_to_2500 LIMIT 1001");
        assert_eq!(received, 1000);
        assert_eq!(done["truncated"], true);

        // Limite écrite : elle s'applique telle quelle, au-delà de la limite par défaut.
        let (columns, received, done) = query(
            &mut socket,
            json!({ "sql": "SELECT seq FROM seq_1_to_2500 LIMIT 1500" }),
        )
        .await;
        assert_eq!(columns["limit"], "user_limit");
        assert_eq!(received, 1500);
        assert_eq!(done["truncated"], false);

        // Une erreur n'interrompt pas la session.
        let (_, _, error) = query(&mut socket, json!({ "sql": "SELECT * FROM missing" })).await;
        assert_eq!(error["type"], "error");
        let (_, _, error) = query(&mut socket, json!({ "sql": "SELECT 1; SELECT 2" })).await;
        assert_eq!(error["type"], "error");
        let (_, received, done) = query(&mut socket, json!({ "sql": "SELECT 1" })).await;
        assert_eq!((received, done["type"].as_str()), (1, Some("done")));
    }
}
