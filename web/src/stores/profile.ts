/**
 * Profile of the user: names and email address, read from the server.
 */

import { defineStore } from 'pinia'
import { ref } from 'vue'

import { getProfile, updateProfile } from '@/api/profile'
import type { Profile } from '@/api/generated/Profile'
import type { ProfileUpdate } from '@/api/generated/ProfileUpdate'

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

  return { profile, load, save }
})
