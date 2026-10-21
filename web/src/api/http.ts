/**
 * HTTP client of the Prisme API.
 *
 * All the routes answer in JSON. On failure, the server returns
 * `{"error": "message", "detail": {...}}` (`ErrorBody` type): the thrown
 * error carries the message that the interface writes from the detail
 * (`messages.ts`), ready to be shown to the user. An answer that is not
 * Prisme's JSON (a refusal of the HTTP layer) is quoted with its status.
 */

import type { ErrorBody } from './generated/ErrorBody'
import type { ErrorDetail } from './generated/ErrorDetail'
import { describe, quoted } from './messages'
import { trackRead, trackWrite } from './sync'

/**
 * Error returned by the API, with its HTTP status.
 */
export class ApiError extends Error {
  /**
   * @param status HTTP status of the response (400, 404, 502...).
   * @param message Message for the user.
   * @param detail What went wrong, as the server told it; absent for an
   *     answer that is not Prisme's JSON.
   */
  constructor(
    readonly status: number,
    message: string,
    readonly detail: ErrorDetail | null = null,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

/**
 * HTTP methods used by the API.
 */
type Method = 'GET' | 'POST' | 'PUT' | 'DELETE'

/**
 * Options of a request.
 */
export interface RequestOptions {
  /**
   * Lets the request outlive the page, for a save made as the page closes.
   * Such a request is limited in size by the browser.
   */
  keepalive?: boolean
  /**
   * Whether the request reads or writes the internal database (true by
   * default): it then shows in the synchronisation state of the top bar,
   * reading for a GET, writing otherwise.
   *
   * `false` for the requests that work elsewhere: on a target server (probe
   * of a connection), or on the server's own data (version, keywords of a
   * dialect, analysis or beautification of SQL). Counted, they would show a
   * synchronisation that is not one, and the failure of a probe (a target
   * server out of reach) would show as a failed save of the user's data.
   */
  sync?: boolean
}

/**
 * Sends a request to the API and returns the JSON body of the response.
 *
 * @param method HTTP method.
 * @param path Path of the route, for example `/api/connections`.
 * @param body Body to send as JSON, absent for a GET.
 * @param options Options of the request.
 * @returns The body of the response, `undefined` if it is empty.
 * @throws {ApiError} If the server answers with an error status.
 */
export async function request<T>(
  method: Method,
  path: string,
  body?: unknown,
  options: RequestOptions = {},
): Promise<T> {
  const sent = send<T>(method, path, body, options)
  if (options.sync === false) {
    return sent
  }
  return method === 'GET' ? trackRead(sent) : trackWrite(sent)
}

/**
 * Sends a request to the API, without tracking it.
 *
 * @param method HTTP method.
 * @param path Path of the route.
 * @param body Body to send as JSON, absent for a GET.
 * @param options Options of the request.
 * @returns The body of the response, `undefined` if it is empty.
 * @throws {ApiError} If the server answers with an error status.
 */
async function send<T>(
  method: Method,
  path: string,
  body: unknown,
  options: RequestOptions,
): Promise<T> {
  const response = await fetch(path, {
    method,
    keepalive: options.keepalive === true,
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
 * Turns an error response into an `ApiError`: the message written from the
 * server's detail, or, for an answer that is not Prisme's JSON, its status
 * and its text, quoted.
 *
 * @param response Response whose status is an error.
 */
export async function toApiError(response: Response): Promise<ApiError> {
  const text = await response.text()
  const detail = detailOf(text)
  if (detail !== null) {
    return new ApiError(response.status, describe(detail), detail)
  }
  const refusal = `Le serveur Prisme a refusé la requête (statut ${response.status})`
  const message = text.trim() === '' ? `${refusal}.` : quoted(refusal, text.trim())
  return new ApiError(response.status, message)
}

/**
 * Detail of an error body of Prisme, `null` if the body is not one.
 *
 * @param text Body of the response.
 */
function detailOf(text: string): ErrorDetail | null {
  try {
    return (JSON.parse(text) as Partial<ErrorBody>).detail ?? null
  } catch {
    return null
  }
}
