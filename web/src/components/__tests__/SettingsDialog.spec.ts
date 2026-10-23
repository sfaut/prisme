import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { Settings } from '@/api/generated/Settings'
import type { SettingsUpdate } from '@/api/generated/SettingsUpdate'

import SettingsDialog from '../SettingsDialog.vue'

/**
 * Settings saved on the server: read-only sessions, `DELETE` allowed.
 */
const SAVED: Settings = {
  row_limit: 1000,
  row_limit_choices: [1000, 2000],
  read_only_session: true,
  allowed_writes: ['delete'],
  history_retention_days: 30,
  debug_retention_days: 30,
  indent_size: 4,
}

const api = vi.hoisted(() => ({
  getSettings: vi.fn<() => Promise<Settings>>(),
  updateSettings: vi.fn<(update: SettingsUpdate) => Promise<Settings>>(),
}))
vi.mock('@/api/settings', () => api)

/**
 * Checkbox of a kind of write, found by its statement.
 *
 * @param statement Statement of the kind (`INSERT`, `CREATE TABLE`...).
 */
function writeBox(statement: string): HTMLInputElement {
  const box = document.querySelector<HTMLInputElement>(`input[aria-label="${statement}"]`)
  if (box === null) {
    throw new Error(`no checkbox ${statement}`)
  }
  return box
}

/**
 * Checkbox of the read-only sessions.
 */
function readOnlyBox(): HTMLInputElement {
  const label = [...document.querySelectorAll('label')].find((element) =>
    element.textContent?.includes('Sessions en lecture seule'),
  )
  const box = label?.querySelector<HTMLInputElement>('input[type="checkbox"]')
  if (box === undefined || box === null) {
    throw new Error('no read-only checkbox')
  }
  return box
}

describe('settings of the writes', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    document.body.innerHTML = ''
    api.getSettings.mockResolvedValue({ ...SAVED })
    api.updateSettings.mockImplementation(async (update) => ({ ...SAVED, ...update }))
  })

  it('greys the writes out while the sessions are read-only, then saves those chosen', async () => {
    const wrapper = mount(SettingsDialog, { props: { open: false }, attachTo: document.body })
    await wrapper.setProps({ open: true })
    await flushPromises()

    expect(writeBox('DELETE').checked).toBe(true)
    expect(writeBox('INSERT').matches(':disabled')).toBe(true)

    readOnlyBox().click()
    await flushPromises()
    expect(writeBox('INSERT').matches(':disabled')).toBe(false)

    writeBox('CREATE TABLE').click()
    document.querySelector<HTMLFormElement>('#settings-form')?.requestSubmit()
    await flushPromises()

    expect(api.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        read_only_session: false,
        allowed_writes: ['delete', 'create_table'],
      }),
    )
  })
})
