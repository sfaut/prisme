/**
 * Messages of the errors sent by the server, written by the interface.
 *
 * The server tells what went wrong as data (`ErrorDetail`: a code and its
 * parameters); the interface writes the message, in the user's language.
 * Two kinds of texts make a message:
 *
 * - those that Prisme writes, here, in French (the interface becomes
 *   multilingual later);
 * - those that come from elsewhere (the target server's answer, the driver's
 *   or the system's message, the SQL parser's), quoted as they are, in their
 *   own language, after a sentence that says who wrote them.
 *
 * The `switch` on the codes is exhaustive: `vue-tsc` refuses a code without
 * its message.
 */

import type { ConnectFailure } from './generated/ConnectFailure'
import type { ErrorDetail } from './generated/ErrorDetail'
import type { FailureCause } from './generated/FailureCause'
import type { Field } from './generated/Field'
import type { Flavor } from './generated/Flavor'
import type { LocationError } from './generated/LocationError'
import { FLAVORS } from './flavors'
import { WRITE_KIND_STATEMENTS } from './writeKinds'

/**
 * Label of each field, as the forms show it.
 */
const FIELD_LABELS: Record<Field, string> = {
  name: 'Nom',
  host: 'Hôte',
  username: 'Compte',
  database_name: 'Base',
  port: 'Port',
  init_sql: 'À la connexion',
  session_timeout_minutes: 'Session gardée',
  first_name: 'Prénom',
  last_name: 'Nom',
  history_retention_days: 'Rétention historique',
  debug_retention_days: 'Rétention debug',
  indent_size: 'Indentation',
  concat_max_bytes: 'GROUP_CONCAT',
  pivot_refresh_ms: 'Mise à jour du Pivoteur',
  remote_connect_timeout_seconds: 'Délai de connexion',
  remote_download_timeout_minutes: 'Durée d’un téléchargement',
  remote_max_mebibytes: 'Taille la plus grande',
  remote_cache_days: 'Copies inutilisées',
  schema_name: 'Schéma',
}

/**
 * Unit of the numbers of a field, with its leading space, if it has one.
 */
const FIELD_UNITS: Partial<Record<Field, string>> = {
  session_timeout_minutes: ' minutes',
  history_retention_days: ' jours',
  debug_retention_days: ' jours',
  indent_size: ' espaces',
  concat_max_bytes: ' octets',
  pivot_refresh_ms: ' millisecondes',
  remote_connect_timeout_seconds: ' secondes',
  remote_download_timeout_minutes: ' minutes',
  remote_max_mebibytes: ' Mio',
  remote_cache_days: ' jours',
}

/**
 * Number in French notation (`50 000`).
 *
 * @param value Number.
 */
function number(value: number): string {
  return value.toLocaleString('fr-FR')
}

/**
 * Text written outside Prisme, introduced by a sentence that says who wrote it.
 *
 * @param intro Sentence that introduces the text, without its colon.
 * @param text Text quoted as it is.
 */
export function quoted(intro: string, text: string): string {
  return `${intro} :\n\n${text}`
}

/**
 * Answer of the target server, or message of the driver or of the system,
 * introduced.
 *
 * @param cause Text and author of the failure.
 * @param flavor Flavor of the server, when known.
 */
function causeText(cause: FailureCause, flavor: Flavor | null = null): string {
  switch (cause.source) {
    case 'server': {
      const server = flavor === null ? 'Le serveur' : `Le serveur ${FLAVORS[flavor].name}`
      const reference = `erreur ${cause.number}, SQLSTATE ${cause.sqlstate}`
      return quoted(`${server} a répondu (${reference})`, cause.message)
    }
    case 'engine': {
      const engine = flavor === null ? 'Le moteur' : FLAVORS[flavor].name
      return quoted(`${engine} a répondu (${cause.name}, code ${cause.code})`, cause.message)
    }
    case 'driver': {
      const driver = flavor === null ? '' : ` (${FLAVORS[flavor].driver})`
      return quoted(`Le pilote de connexion${driver} a signalé`, cause.message)
    }
    case 'system':
      return quoted('Le système a signalé', cause.message)
  }
}

/**
 * Message of Prisme, followed by the text that explains it.
 *
 * @param sentence Sentence of Prisme.
 * @param cause Text and author of the failure.
 * @param flavor Flavor of the server, when known.
 */
function withCause(sentence: string, cause: FailureCause, flavor: Flavor | null = null): string {
  return `${sentence}\n\n${causeText(cause, flavor)}`
}

/**
 * Message of a failed connection.
 *
 * @param failure Why the connection could not open.
 */
export function connectMessage(failure: ConnectFailure): string {
  switch (failure.reason) {
    case 'refused':
      return `${failure.address} refuse la connexion : aucun serveur n’écoute sur ce port, ou il n’accepte que les connexions locales (bind-address).`
    case 'unreachable':
      return `${failure.address} est injoignable : serveur arrêté, adresse erronée ou pare-feu.`
    case 'timeout':
      return `${failure.address} ne répond pas (délai de ${failure.seconds} s dépassé) : serveur arrêté, adresse ou port erronés, ou pare-feu.`
    case 'access_denied':
      return withCause(
        `Accès refusé au compte « ${failure.username} » : mot de passe erroné, ou compte non autorisé depuis cette machine.`,
        failure.cause,
      )
    case 'host_not_allowed':
      return withCause(
        `Le serveur refuse les connexions de cette machine : aucun compte « ${failure.username} » n’y est autorisé depuis cette adresse.`,
        failure.cause,
      )
    case 'unknown_database':
      return `La base « ${failure.database} » n’existe pas sur ${failure.address}.`
    case 'database_access_denied':
      return `Le compte « ${failure.username} » n’a pas accès à la base « ${failure.database} ».`
    case 'server_refused':
      return withCause(`${failure.address} refuse la connexion.`, failure.cause)
    case 'unsupported_authentication':
      return `Le compte « ${failure.username} » utilise une méthode d’authentification que Prisme ne prend pas en charge (${failure.plugin}).`
    case 'file_not_found':
      return `Le fichier ${failure.location} n’existe pas.`
    case 'not_a_database':
      return withCause(`${failure.location} n’est pas une base de données.`, failure.cause)
    case 'file_unreadable':
      return withCause(`Impossible d’ouvrir ${failure.location}.`, failure.cause)
    case 'location':
      return locationMessage(failure.error)
    case 'other':
      return withCause(`Connexion impossible à ${failure.address}.`, failure.cause)
  }
}

/**
 * Message of a location refused, or that could not be reached.
 *
 * @param error Why the location is refused or unreachable.
 */
export function locationMessage(error: LocationError): string {
  switch (error.code) {
    case 'empty':
      return 'Aucun emplacement n’est saisi.'
    case 'too_long':
      return `L’emplacement dépasse ${error.max} caractères.`
    case 'relative_path':
      return `${error.address} : le chemin doit être absolu.`
    case 'invalid_url':
      return `${error.address} : adresse illisible.`
    case 'credentials':
      return `${error.address} : un emplacement web doit être public, sans compte.`
    case 'not_served':
      return `Le protocole ${error.scheme} n’est pas encore supporté par Prisme.`
    case 'unknown_scheme':
      return `Le protocole ${error.scheme} est inconnu.`
    case 'already_exists':
      return `${error.address} existe déjà.`
    case 'too_large':
      return `Le fichier dépasse ${error.max_bytes} octets, la taille permise par les préférences.`
    case 'not_creatable':
      return `Prisme ne peut pas créer de base par le protocole ${error.scheme}.`
    case 'unreachable':
      return quoted(`${error.address} est injoignable`, error.message)
  }
}

/**
 * Message of an error sent by the server.
 *
 * @param detail What went wrong.
 */
export function describe(detail: ErrorDetail): string {
  switch (detail.code) {
    case 'not_found':
      return 'Élément introuvable : il a peut-être été supprimé.'
    case 'internal':
      return 'Erreur interne de Prisme : le détail est dans le journal du serveur.'
    case 'no_connection':
      return 'Aucune connexion n’est choisie pour cet éditeur.'
    case 'field_length': {
      const label = FIELD_LABELS[detail.field]
      const bounds =
        detail.min === 0
          ? `${number(detail.max)} caractères au plus`
          : `entre ${number(detail.min)} et ${number(detail.max)} caractères`
      return `${label} : ${bounds}, ${number(detail.length)} reçus.`
    }
    case 'field_range': {
      const unit = FIELD_UNITS[detail.field] ?? ''
      return `${FIELD_LABELS[detail.field]} : valeur attendue entre ${number(detail.min)} et ${number(detail.max)}${unit}.`
    }
    case 'row_limit_not_allowed':
      return `Limite de ${number(detail.value)} lignes non proposée : choisir parmi ${detail.choices.map(number).join(', ')}.`
    case 'duplicate_connection_name':
      return `Une connexion nommée « ${detail.name} » existe déjà.`
    case 'settings_family':
      return `Les réglages envoyés ne conviennent pas au moteur « ${detail.engine} ».`
    case 'main_database_first':
      return 'La première base est la base principale, de schéma « main ».'
    case 'too_many_databases':
      return `${number(detail.max)} bases attachées au plus, en plus de la base principale.`
    case 'duplicate_schema':
      return `Le schéma « ${detail.schema} » est donné deux fois (sans tenir compte de la casse).`
    case 'reserved_schema':
      return `Le schéma « ${detail.schema} » est réservé par le moteur : choisir un autre nom.`
    case 'location':
      return `Schéma « ${detail.schema} » : ${locationMessage(detail.error)}`
    case 'init_sql_not_understood':
      return `À la connexion : Prisme ne comprend pas la requête « ${detail.statement} ».`
    case 'init_sql_global_setting':
      return `À la connexion : « ${detail.statement} » est un réglage global, qui changerait tout le serveur ; seuls les réglages de session sont permis.`
    case 'init_sql_not_setting':
      return `À la connexion : seuls les réglages de session (SET …) sont permis, « ${detail.statement} » en est un autre.`
    case 'connect':
      return connectMessage(detail.failure)
    case 'missing_version':
      return 'Le serveur n’a pas donné sa version.'
    case 'init_query_failed':
      return withCause(`Échec de la requête de connexion « ${detail.statement} ».`, detail.cause)
    case 'query_failed':
      return causeText(detail.cause, detail.flavor)
    case 'catalog_unreadable': {
      const what = detail.schema === null ? 'liste des bases' : `base « ${detail.schema} »`
      return withCause(`Lecture du catalogue impossible (${what}).`, detail.cause, detail.flavor)
    }
    case 'query_interrupted':
      return 'Requête interrompue par l’utilisateur.'
    case 'session_closed':
      return 'Session fermée avant la fin de la requête.'
    case 'connection_closed_by_server':
      return 'Le serveur a fermé la connexion de la session (COMMIT RELEASE ou ROLLBACK RELEASE, completion_type = 2, KILL, arrêt ou redémarrage du serveur…) : « Reconnecter », ou une requête exécutée, en ouvre une nouvelle.'
    case 'no_query':
      return 'Aucune requête à exécuter.'
    case 'multiple_queries':
      return `${number(detail.count)} requêtes reçues : une seule à la fois.`
    case 'query_skipped':
      return 'Requête non exécutée : une requête précédente du lot a échoué.'
    case 'write_read_only':
      return 'Écriture refusée : les sessions sont en lecture seule (Paramètres, « Sessions en lecture seule »).'
    case 'write_not_allowed': {
      const statements = detail.kinds.map((kind) => WRITE_KIND_STATEMENTS[kind]).join(', ')
      return detail.kinds.length > 1
        ? `Écriture refusée : ${statements} ne sont pas autorisés dans les Paramètres.`
        : `Écriture refusée : ${statements} n’est pas autorisé dans les Paramètres.`
    }
    case 'attach_path_in_memory':
      return `ATTACH refusé : la base principale de la connexion est en mémoire (memdb), où le chemin ${detail.path} ouvrirait une base vide au lieu du fichier. Nommer le fichier par son URI : ATTACH '${detail.uri}' AS …`
    case 'write_forbidden':
      return 'Écriture refusée : aucun paramètre n’autorise cette instruction (appel de procédure, base de données, déclencheur, compte, fichier, réglage du serveur…).'
    case 'read_write_transaction_read_only':
      return 'Transaction refusée : elle lèverait la lecture seule des sessions (Paramètres, « Sessions en lecture seule »). START TRANSACTION et BEGIN restent possibles.'
    case 'xa_transaction_unsupported':
      return 'Requêtes XA non prises en charge par Prisme (transactions distribuées : XA START, XA PREPARE, XA COMMIT…).'
    case 'unparsed_query_refused':
      return 'Requête refusée : Prisme ne sait pas l’analyser, il ne peut donc pas vérifier qu’elle est une lecture.'
    case 'settings_unreadable':
      return 'Paramètres de l’utilisateur illisibles : le détail est dans le journal du serveur.'
    case 'invalid_message':
      return `Le serveur ne comprend pas un message de l’interface.\n\n${quoted('Le lecteur JSON a signalé', detail.cause)}`
    case 'query_not_understood':
      return `Requête non comprise par Prisme.\n\n${quoted('L’analyseur SQL a signalé', detail.cause)}`
    case 'export_not_read':
      return 'Export refusé : seule une lecture (SELECT, SHOW…) a des lignes à exporter.'
    case 'duplicate_columns':
      return `Export refusé : colonnes de même nom (${detail.names.join(', ')}).`
    case 'export_file':
      return `Fichier d’export non créé, non écrit ou non supprimé : il est peut-être ouvert dans un autre programme.\n\n${quoted('Le système a signalé', detail.cause)}`
    case 'export_file_missing':
      return `Fichier d’export absent du dossier des exports (${detail.file}) : il a été supprimé ou déplacé hors de Prisme.`
  }
}
