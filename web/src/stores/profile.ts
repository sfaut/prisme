/**
 * Profile of the user: names and email address, read from the server, and
 * the initials shown in the avatar of the top bar.
 */

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import { getProfile, updateProfile } from '@/api/profile'
import type { Profile } from '@/api/generated/Profile'
import type { ProfileUpdate } from '@/api/generated/ProfileUpdate'

/**
 * Initials of a user: those of the first and last names, or, without names,
 * the first letter of the email address; empty without any of them.
 *
 * @param profile Profile of the user.
 */
export function initialsOf(profile: Profile): string {
  const first = (text: string) => Array.from(text.trim())[0] ?? ''
  const names = `${first(profile.first_name)}${first(profile.last_name)}`
  return (names === '' ? first(profile.email) : names).toLocaleUpperCase('fr-FR')
}

export const useProfileStore = defineStore('profile', () => {
  /**
   * Profile read from the server, `null` before the first load.
   */
  const profile = ref<Profile | null>(null)

  /**
   * Loads the profile from the server.
   */
  async function load(): Promise<void> {
    profile.value = await getProfile()
  }

  /**
   * Changes the names of the user.
   *
   * @param update New first and last names.
   * @throws {ApiError} If a name is too long.
   */
  async function save(update: ProfileUpdate): Promise<void> {
    profile.value = await updateProfile(update)
  }

  /**
   * Initials of the user, empty before the first load.
   */
  const initials = computed(() => (profile.value === null ? '' : initialsOf(profile.value)))

  /**
   * Name of the user as shown: first and last names, or the email address.
   */
  const displayName = computed(() => {
    const value = profile.value
    if (value === null) {
      return ''
    }
    const names = `${value.first_name} ${value.last_name}`.trim()
    return names === '' ? value.email : names
  })

  return { profile, initials, displayName, load, save }
})
