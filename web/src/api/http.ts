/**
 * Client HTTP de l'API Prisme.
 *
 * Toutes les routes répondent en JSON. En cas d'échec, le serveur renvoie
 * `{"error": "message"}` (type `ErrorBody`) : ce message devient celui de
 * l'erreur levée, prêt à être affiché à l'utilisateur.
 */

import type { ErrorBody } from './generated/ErrorBody'

/**
 * Erreur renvoyée par l'API, avec son statut HTTP.
 */
export class ApiError extends Error {
  /**
   * @param status Statut HTTP de la réponse (400, 404, 502...).
   * @param message Message lisible, fourni par le serveur.
   */
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

/**
 * Méthodes HTTP utilisées par l'API.
 */
type Method = 'GET' | 'POST' | 'PUT' | 'DELETE'

/**
 * Envoie une requête à l'API et renvoie le corps JSON de la réponse.
 *
 * @param method Méthode HTTP.
 * @param path Chemin de la route, par exemple `/api/connections`.
 * @param body Corps à envoyer en JSON, absent pour un GET.
 * @returns Le corps de la réponse, `undefined` s'il est vide.
 * @throws {ApiError} Si le serveur répond par un statut d'erreur.
 */
export async function request<T>(method: Method, path: string, body?: unknown): Promise<T> {
  const response = await fetch(path, {
    method,
    headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (!response.ok) {
    throw await toApiError(response)
  }
  const text = await response.text()
  return (text === '' ? undefined : JSON.parse(text)) as T
}

/**
 * Transforme une réponse en erreur en `ApiError`, avec le message du serveur.
 *
 * @param response Réponse dont le statut est une erreur.
 */
export async function toApiError(response: Response): Promise<ApiError> {
  return new ApiError(response.status, errorMessage(response, await response.text()))
}

/**
 * Extrait le message d'une réponse en erreur : celui du serveur s'il est
 * lisible, sinon le statut HTTP.
 *
 * @param response Réponse en erreur.
 * @param text Corps de la réponse.
 */
function errorMessage(response: Response, text: string): string {
  try {
    return (JSON.parse(text) as ErrorBody).error
  } catch {
    return `${response.status} ${response.statusText}`.trim()
  }
}
