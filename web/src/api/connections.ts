/**
 * Connexions enregistrées : liste, création, modification, suppression et test.
 */

import { request } from './http'
import type { ConnectionSummary } from './generated/ConnectionSummary'
import type { Created } from './generated/Created'
import type { NewConnection } from './generated/NewConnection'
import type { ProbeRequest } from './generated/ProbeRequest'
import type { ServerInfo } from './generated/ServerInfo'

/**
 * Liste les connexions de l'utilisateur, sans mot de passe.
 */
export function listConnections(): Promise<ConnectionSummary[]> {
  return request('GET', '/api/connections')
}

/**
 * Enregistre une connexion ; le mot de passe est chiffré côté serveur.
 *
 * @param connection Connexion saisie par l'utilisateur.
 * @returns L'identifiant de la connexion créée.
 */
export function createConnection(connection: NewConnection): Promise<Created> {
  return request('POST', '/api/connections', connection)
}

/**
 * Modifie une connexion. Un mot de passe vide garde le mot de passe enregistré.
 *
 * @param id Identifiant de la connexion.
 * @param connection Connexion saisie par l'utilisateur.
 */
export function updateConnection(id: number, connection: NewConnection): Promise<void> {
  return request('PUT', `/api/connections/${id}`, connection)
}

/**
 * Supprime une connexion.
 *
 * @param id Identifiant de la connexion.
 */
export function deleteConnection(id: number): Promise<void> {
  return request('DELETE', `/api/connections/${id}`)
}

/**
 * Se connecte au serveur et renvoie sa variante (MySQL ou MariaDB) et sa version.
 *
 * @param id Identifiant de la connexion.
 */
export function probeConnection(id: number): Promise<ServerInfo> {
  return request('POST', `/api/connections/${id}/probe`)
}

/**
 * Se connecte avec des paramètres saisis, pas encore enregistrés, et renvoie
 * la variante et la version du serveur.
 *
 * @param probe
 *     Paramètres saisis, et connexion enregistrée en cours de modification
 *     dont reprendre le mot de passe si le champ est vide.
 */
export function probeSettings(probe: ProbeRequest): Promise<ServerInfo> {
  return request('POST', '/api/connections/probe', probe)
}

/**
 * Variante et version du serveur, telles qu'affichées (`MariaDB 11.4.2-MariaDB`).
 *
 * @param info Identité du serveur.
 */
export function serverLabel(info: ServerInfo): string {
  return `${info.flavor === 'mariadb' ? 'MariaDB' : 'MySQL'} ${info.version}`
}
