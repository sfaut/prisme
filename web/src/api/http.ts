/**
 * HTTP client of the Prisme API.
 *
 * All the routes answer in JSON. On failure, the server returns
 * `{"error": "message"}` (`ErrorBody` type): this message becomes that of the
 * thrown error, ready to be shown to the user.
 */

import type { ErrorBody } from './generated/ErrorBody'
import { trackWrite } from './sync'

/**
 * Error returned by the API, with its HTTP status.
 */
export class ApiError extends Error {
  /**
   * @param status HTTP status of the response (400, 404, 502...).
   * @param message Readable message, provided by the server.
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
 * HTTP methods used by the API.
 */
type Method = 'GET' | 'POST' | 'PUT' | 'DELETE'

/**
 * Sends a request to the API and returns the JSON body of the response.
 *
 * @param method HTTP method.
 * @param path Path of the route, for example `/api/connections`.
 * @param body Body to send as JSON, absent for a GET.
 * @returns The body of the response, `undefined` if it is empty.
 * @throws {ApiError} If the server answers with an error status.
 */
export async function request<T>(method: Method, path: string, body?: unknown): Promise<T> {
  const sent = send<T>(method, path, body)
  return method === 'GET' ? sent : trackWrite(sent)
}

/**
 * Sends a request to the API, without tracking it.
 *
 * @param method HTTP method.
 * @param path Path of the route.
 * @param body Body to send as JSON, absent for a GET.
 * @returns The body of the response, `undefined` if it is empty.
 * @throws {ApiError} If the server answers with an error status.
 */
async function send<T>(method: Method, path: string, body?: unknown): Promise<T> {
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
 * Turns an error response into an `ApiError`, with the server's message.
 *
 * @param response Response whose status is an error.
 */
export async function toApiError(response: Response): Promise<ApiError> {
  return new ApiError(response.status, errorMessage(response, await response.text()))
}

/**
 * Extracts the message of an error response: the server's if it is readable,
 * otherwise the HTTP status.
 *
 * @param response Error response.
 * @param text Body of the response.
 */
function errorMessage(response: Response, text: string): string {
  try {
    return (JSON.parse(text) as ErrorBody).error
  } catch {
    return `${response.status} ${response.statusText}`.trim()
  }
}
