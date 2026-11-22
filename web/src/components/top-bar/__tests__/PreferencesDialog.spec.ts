import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { Settings } from '@/api/generated/Settings'
import type { SettingsUpdate } from '@/api/generated/SettingsUpdate'

import PreferencesDialog from '../PreferencesDialog.vue'

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
  concat_max_bytes: 1000,
  pivot_refresh_ms: 1000,
  remote_connect_timeout_seconds: 10,
  remote_download_timeout_minutes: 5,
  remote_max_mebibytes: 1024,
  remote_cache_days: 30,
}

const settingsApi = vi.hoisted(() => ({
  getSettings: vi.fn<() => Promise<Settings>>(),
  updateSettings: vi.fn<(update: SettingsUpdate) => Promise<Settings>>(),
}))
vi.mock('@/api/settings', () => settingsApi)

/**
 * Opens the window on the saved values, with the given settings.
 *
 * @param settings Settings saved on the server.
 */
async function openWindow(settings: Settings = SAVED): Promise<void> {
  settingsApi.getSettings.mockResolvedValue({ ...settings })
  const wrapper = mount(PreferencesDialog, { props: { open: false }, attachTo: document.body })
  await wrapper.setProps({ open: true })
  await flushPromises()
}

/**
 * Shows a section of the window, by the name of its tab.
 *
 * @param name Name of the section ("Requêtes"...).
 */
async function showSection(name: string): Promise<void> {
  const tab = [...document.querySelectorAll<HTMLElement>('[role="tab"]')].find(
    (element) => element.textContent?.trim() === name,
  )
  if (tab === undefined) {
    throw new Error(`no section ${name}`)
  }
  tab.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }))
  await flushPromises()
}

/**
 * Element of the window found by a selector.
 *
 * @param selector CSS selector of the element.
 */
function find<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector)
  if (element === null) {
    throw new Error(`nothing matches ${selector}`)
  }
  return element
}

/**
 * Checkbox of a kind of write, found by its statement.
 *
 * @param statement Statement of the kind (`INSERT`, `CREATE TABLE`...).
 */
function writeBox(statement: string): HTMLInputElement {
  return find<HTMLInputElement>(`input[aria-label="${statement}"]`)
}

/**
 * Types a value in a field, as the keyboard would.
 *
 * @param field Field typed in.
 * @param value Value typed.
 */
function type(field: HTMLInputElement, value: string): void {
  field.value = value
  field.dispatchEvent(new Event('input'))
}

/**
 * Submits the window, as its "Enregistrer" button does.
 */
async function submit(): Promise<void> {
  find<HTMLFormElement>('#preferences-form').requestSubmit()
  await flushPromises()
}

beforeEach(() => {
  setActivePinia(createPinia())
  document.body.innerHTML = ''
  vi.clearAllMocks()
  settingsApi.updateSettings.mockImplementation(async (update) => ({ ...SAVED, ...update }))
})

describe('writes', () => {
  it('greys the writes out while the sessions are read-only, then saves those chosen', async () => {
    await openWindow()
    await showSection('Requêtes')

    expect(writeBox('DELETE').checked).toBe(true)
    expect(writeBox('INSERT').matches(':disabled')).toBe(true)

    find<HTMLButtonElement>('#preferences-read-only').click()
    await flushPromises()
    expect(writeBox('INSERT').matches(':disabled')).toBe(false)

    writeBox('CREATE TABLE').click()
    await submit()

    expect(settingsApi.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        read_only_session: false,
        allowed_writes: ['delete', 'create_table'],
      }),
    )
  })

  it('keeps each verb in its column, an object without the verb leaving its cell empty', async () => {
    await openWindow()
    await showSection('Requêtes')

    const column = (statement: string): string | undefined =>
      [...(writeBox(statement).parentElement?.classList ?? [])].find((name) =>
        name.startsWith('col-start-'),
      )
    expect(column('DROP VIEW')).toBe(column('DROP TABLE'))
    expect(column('DROP EVENT')).toBe(column('DROP TABLE'))
    expect(column('ALTER EVENT')).toBe(column('ALTER TABLE'))
    expect(column('TRUNCATE TABLE')).toBe(column('DROP TABLE'))
  })
})

describe('longest text of a GROUP_CONCAT', () => {
  it('shows the saved length, then saves the one typed', async () => {
    await openWindow({ ...SAVED, concat_max_bytes: 2000 })
    await showSection('Pivoteur')

    const field = find<HTMLInputElement>('#preferences-concat')
    expect(field.value).toBe('2000')
    expect([field.min, field.max]).toEqual(['100', '1048576'])

    type(field, '4000')
    await submit()

    expect(settingsApi.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({ concat_max_bytes: 4000 }),
    )
  })
})

describe('delay between two computations of the Pivoteur', () => {
  it('shows the saved delay in seconds, then saves the one typed in milliseconds', async () => {
    await openWindow({ ...SAVED, pivot_refresh_ms: 2500 })
    await showSection('Pivoteur')

    const field = find<HTMLInputElement>('#preferences-refresh')
    expect(field.value).toBe('2.5')
    expect([field.min, field.max, field.step]).toEqual(['0.2', '60', '0.1'])

    type(field, '0.3')
    await submit()

    expect(settingsApi.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({ pivot_refresh_ms: 300 }),
    )
  })
})

describe('reading of the remote databases', () => {
  it('shows the saved limits, then saves those typed', async () => {
    await openWindow({ ...SAVED, remote_max_mebibytes: 2048 })
    await showSection('Bases distantes')

    const size = find<HTMLInputElement>('#preferences-remote-size')
    expect(size.value).toBe('2048')
    expect([size.min, size.max]).toEqual(['1', '102400'])
    expect(find<HTMLInputElement>('#preferences-remote-connect').value).toBe('10')

    type(size, '512')
    type(find<HTMLInputElement>('#preferences-remote-cache'), '7')
    await submit()

    expect(settingsApi.updateSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        remote_connect_timeout_seconds: 10,
        remote_download_timeout_minutes: 5,
        remote_max_mebibytes: 512,
        remote_cache_days: 7,
      }),
    )
  })
})
