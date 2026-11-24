import { flushPromises, mount } from '@vue/test-utils'
import { MYSQL_CAPABILITIES } from '@/api/__tests__/capabilities'
import { describe, expect, it, vi } from 'vitest'

import type { EngineInfo } from '@/api/generated/EngineInfo'

import ConnectionFormFiles from '../ConnectionFormFiles.vue'
import { emptyForm } from '../connectionForm'

const api = vi.hoisted(() => ({
  fetchEngines: vi.fn<() => Promise<EngineInfo[]>>(),
}))
vi.mock('@/api/engines', () => api)

/**
 * SQLite as the server lists it: two pragmas of the connection allowed.
 */
const SQLITE: EngineInfo = {
  engine: 'sqlite',
  flavors: ['sqlite'],
  capabilities: { ...MYSQL_CAPABILITIES, session_settings: ['busy_timeout', 'foreign_keys'] },
}

/**
 * Text of the form of a new SQLite connection, once the engines are read.
 */
async function formText(): Promise<string> {
  const wrapper = mount(ConnectionFormFiles, { props: { form: emptyForm('sqlite') } })
  await flushPromises()
  return wrapper.text()
}

describe('ConnectionFormFiles', () => {
  it('states beside the queries run on connection the pragmas they may change', async () => {
    api.fetchEngines.mockResolvedValue([SQLITE])
    expect(await formText()).toContain('Seuls ceux-ci sont permis : busy_timeout, foreign_keys.')
  })

  it('names no pragma while the engines are unknown', async () => {
    api.fetchEngines.mockRejectedValue(new Error('unreachable'))
    const text = await formText()
    expect(text).toContain('Pragmas exécutés à l’ouverture de chaque connexion')
    expect(text).not.toContain('permis')
  })
})
