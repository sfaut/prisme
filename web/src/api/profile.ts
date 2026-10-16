/**
 * Profile of the user: names and email address.
 */

import { request } from './http'
import type { Profile } from './generated/Profile'
import type { ProfileUpdate } from './generated/ProfileUpdate'

/**
 * Reads the profile.
 */
export function getProfile(): Promise<Profile> {
  return request('GET', '/api/profile')
}

/**
 * Changes the names and returns the updated profile.
 *
 * @param update New first and last names.
 * @throws {ApiError} If a name is too long.
 */
export function updateProfile(update: ProfileUpdate): Promise<Profile> {
  return request('PUT', '/api/profile', update)
}
