/**
 * Display settings of the user, common to all the editors, saved on the
 * server side (`crates/server/src/user_display.rs`).
 */

import { type RequestOptions, request } from './http'
import type { UserDisplay } from './generated/UserDisplay'

/**
 * Display settings of the user.
 */
export async function getUserDisplay(): Promise<UserDisplay> {
  return request<UserDisplay>('GET', '/api/display')
}

/**
 * Saves the display settings of the user.
 *
 * @param display Whole display settings.
 * @param options Options of the request, such as `keepalive` for a save made as the page closes.
 */
export async function saveUserDisplay(
  display: UserDisplay,
  options: RequestOptions = {},
): Promise<void> {
  await request('PUT', '/api/display', display, options)
}
