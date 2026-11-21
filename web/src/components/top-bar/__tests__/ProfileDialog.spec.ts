import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, expect, it, vi } from 'vitest'

import type { Profile } from '@/api/generated/Profile'
import type { ProfileUpdate } from '@/api/generated/ProfileUpdate'

import ProfileDialog from '../ProfileDialog.vue'

/**
 * Profile saved on the server.
 */
const PROFILE: Profile = { email: 'ada@example.com', first_name: 'Ada', last_name: 'Lovelace' }

const profileApi = vi.hoisted(() => ({
  getProfile: vi.fn<() => Promise<Profile>>(),
  updateProfile: vi.fn<(update: ProfileUpdate) => Promise<Profile>>(),
}))
vi.mock('@/api/profile', () => profileApi)

/**
 * Field of the window found by its id.
 *
 * @param id Id of the field.
 */
function field(id: string): HTMLInputElement {
  const element = document.querySelector<HTMLInputElement>(`#${id}`)
  if (element === null) {
    throw new Error(`no field ${id}`)
  }
  return element
}

beforeEach(() => {
  setActivePinia(createPinia())
  document.body.innerHTML = ''
  vi.clearAllMocks()
  profileApi.getProfile.mockResolvedValue({ ...PROFILE })
  profileApi.updateProfile.mockImplementation(async (update) => ({ ...PROFILE, ...update }))
})

it('shows the saved names and the email address, then saves the names typed', async () => {
  const wrapper = mount(ProfileDialog, { props: { open: false }, attachTo: document.body })
  await wrapper.setProps({ open: true })
  await flushPromises()

  expect(field('profile-first-name').value).toBe('Ada')
  expect(field('profile-last-name').value).toBe('Lovelace')
  expect(document.body.textContent).toContain('ada@example.com')

  field('profile-first-name').value = 'Augusta'
  field('profile-first-name').dispatchEvent(new Event('input'))
  document.querySelector<HTMLFormElement>('#profile-form')?.requestSubmit()
  await flushPromises()

  expect(profileApi.updateProfile).toHaveBeenCalledWith({
    first_name: 'Augusta',
    last_name: 'Lovelace',
  })
  expect(wrapper.emitted('update:open')).toEqual([[false]])
})
