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
}

/**
 * Unit of the numbers of a field, with its leading space, if it has one.
 */
const FIELD_UNITS: Partial<Record<Field, string>> = {
  session_timeout_minutes: ' minutes',
  history_retention_days: ' jours',
  debug_retention_days: ' jours',
}

/**
 * Name of each flavor of server.
 */
const FLAVOR_NAMES: Record<Flavor, string> = {
  mysql: 'MySQL',
  mariadb: 'MariaDB',
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
      const server = flavor === null ? 'Le serveur' : `Le serveur ${FLAVOR_NAMES[flavor]}`
      const reference = `erreur ${cause.number}, SQLSTATE ${cause.sqlstate}`
      return quoted(`${server} a répondu (${reference})`, cause.message)
    }
    case 'driver':
      return quoted('Le pilote de connexion (mysql_async) a signalé', cause.message)
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
function connectMessage(failure: ConnectFailure): string {
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
    case 'other':
      return withCause(`Connexion impossible à ${failure.address}.`, failure.cause)
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
    case 'no_query':
      return 'Aucune requête à exécuter.'
    case 'multiple_queries':
      return `${number(detail.count)} requêtes reçues : une seule à la fois.`
    case 'write_refused':
      return 'Écriture refusée : les écritures restent désactivées tant que les interrupteurs d’écriture n’existent pas.'
    case 'unparsed_query_refused':
      return 'Requête refusée : Prisme ne sait pas l’analyser, il ne peut donc pas vérifier qu’elle est une lecture.'
    case 'settings_unreadable':
      return 'Paramètres de l’utilisateur illisibles : le détail est dans le journal du serveur.'
    case 'invalid_message':
      return `Le serveur ne comprend pas un message de l’interface.\n\n${quoted('Le lecteur JSON a signalé', detail.cause)}`
    case 'query_not_understood':
      return `Requête non comprise par Prisme.\n\n${quoted('L’analyseur SQL a signalé', detail.cause)}`
  }
}
