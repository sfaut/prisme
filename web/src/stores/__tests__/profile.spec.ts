import { describe, expect, it } from 'vitest'

import { initialsOf } from '../profile'

/**
 * Profile with the names `first_name` and `last_name`.
 */
function profile(first_name: string, last_name: string, email = 'ada@example.com') {
  return { email, first_name, last_name, read_only_session: true }
}

describe('initials of the user', () => {
  it('come from the names, else from the email address', () => {
    expect(initialsOf(profile('Ada', 'Lovelace'))).toBe('AL')
    expect(initialsOf(profile(' élodie ', ''))).toBe('É')
    expect(initialsOf(profile('', ''))).toBe('A')
    expect(initialsOf(profile('', '', ''))).toBe('')
  })
})
