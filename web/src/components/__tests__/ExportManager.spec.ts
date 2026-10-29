import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { flushPromises, mount } from '@vue/test-utils'

import type { ExportInfo } from '@/api/generated/ExportInfo'
import type { ExportList } from '@/api/generated/ExportList'

import ExportManager from '../ExportManager.vue'

const listExports = vi.fn<() => Promise<ExportList>>()
const removeOrphans = vi.fn<() => Promise<void>>()

vi.mock('@/api/exports', () => ({
  listExports: () => listExports(),
  removeOrphans: () => removeOrphans(),
  followExports: () => () => {},
}))

/**
 * Running export of 2,000 rows.
 */
const RUNNING: ExportInfo = {
  id: 1,
  name: 'requête-1-20261029-100000.csv',
  connection_name: 'local',
  sql: 'SELECT seq FROM seq_1_to_3000000',
  state: 'running',
  rows: 2000,
  bytes: 2_000_000,
  expected_rows: null,
  created_at: '2026-10-29T10:00:00.000Z',
  ended_at: null,
  error: null,
  file_missing: false,
}

beforeEach(() => {
  setActivePinia(createPinia())
  listExports.mockReset()
  removeOrphans.mockReset()
})

afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Mounts the manager, its list read, then opens it.
 */
async function opened() {
  const wrapper = mount(ExportManager, { attachTo: document.body })
  await flushPromises()
  await wrapper.find('button').trigger('click')
  await flushPromises()
  return wrapper
}

describe('ExportManager', () => {
  it('shows the running exports on its button, the room of the files and the list', async () => {
    listExports.mockResolvedValue({
      exports: [RUNNING],
      orphans: { count: 0, bytes: 0 },
    })
    const wrapper = await opened()
    expect(wrapper.find('button').attributes('aria-label')).toBe('Exports : 1 en cours')
    expect(document.body.textContent).toContain('1 fichier · 2,0 Mo')
    expect(document.body.textContent).toContain('requête-1-20261029-100000.csv')
    expect(document.body.textContent).not.toContain('Non référencés')
  })

  it('counts the files no export references, and removes them', async () => {
    listExports.mockResolvedValue({
      exports: [],
      orphans: { count: 2, bytes: 3_000 },
    })
    removeOrphans.mockResolvedValue()
    await opened()
    expect(document.body.textContent).toContain('Aucun export')
    expect(document.body.textContent).toContain('Non référencés : 2 fichiers · 3,0 ko')

    const remove = [...document.querySelectorAll('button')].find(
      (button) => button.textContent?.trim() === 'Retirer',
    )
    remove?.click()
    await flushPromises()
    expect(removeOrphans).toHaveBeenCalledOnce()
    expect(listExports).toHaveBeenCalledTimes(2)
  })
})
