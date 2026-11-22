import { flushPromises, mount } from '@vue/test-utils'
import { MYSQL_CAPABILITIES } from '@/api/__tests__/capabilities'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import type { NewConnection } from '@/api/generated/NewConnection'
import type { ProbeRequest } from '@/api/generated/ProbeRequest'
import type { SourceInfo } from '@/api/generated/SourceInfo'
import { useConnectionsStore } from '@/stores/connections'

import ConnectionManager from '../ConnectionManager.vue'

/**
 * Server answering the tests.
 */
const SERVER: SourceInfo = {
  flavor: 'mariadb',
  version: '11.4.4-MariaDB',
  ansi_quotes: false,
  no_backslash_escapes: false,
  oldest_supported: null,
  capabilities: MYSQL_CAPABILITIES,
}

/**
 * Connections saved on the simulated server.
 */
let saved: ConnectionSummary[] = []

const api = vi.hoisted(() => ({
  listConnections: vi.fn<() => Promise<ConnectionSummary[]>>(),
  createConnection: vi.fn<(connection: NewConnection) => Promise<{ id: number }>>(),
  copyConnection: vi.fn<(id: number, connection: NewConnection) => Promise<{ id: number }>>(),
  updateConnection: vi.fn<(id: number, connection: NewConnection) => Promise<void>>(),
  deleteConnection: vi.fn<(id: number) => Promise<void>>(),
  probeConnection: vi.fn<(id: number) => Promise<SourceInfo>>(),
  probeSettings: vi.fn<(probe: ProbeRequest) => Promise<SourceInfo>>(),
}))
vi.mock('@/api/connections', async (original) => ({
  ...(await original<typeof import('@/api/connections')>()),
  ...api,
}))

/**
 * A saved connection, known server, never connected.
 *
 * @param id Id of the connection.
 * @param name Name of the connection.
 */
function connection(id: number, name: string): ConnectionSummary {
  return {
    id,
    name,
    engine: 'mysql',
    host: `${name}.lan`,
    port: 3306,
    username: 'root',
    database_name: '',
    init_sql: '',
    session_timeout_minutes: 5,
    show_system_schemas: true,
    server_flavor: 'mariadb',
    server_version: '11.4.4-MariaDB',
    connected_at: null,
    created_at: '2026-11-01T10:00:00.000Z',
  }
}

/**
 * Mounts the manager and opens it.
 *
 * @param target What it opens on.
 */
async function openManager(target: number | 'new' | null = null): Promise<void> {
  mount(ConnectionManager, { attachTo: document.body })
  useConnectionsStore().openManager(target)
  await flushPromises()
}

/**
 * Button of the window, by its text or its label.
 *
 * @param name Text or `aria-label` of the button.
 */
function button(name: string): HTMLButtonElement {
  const found = [...document.querySelectorAll<HTMLButtonElement>('button')].find(
    (element) =>
      element.textContent?.trim() === name || element.getAttribute('aria-label') === name,
  )
  if (found === undefined) {
    throw new Error(`no button ${name}`)
  }
  return found
}

/**
 * Field of the window, by the name of its setting.
 *
 * @param label Name of the setting ("Nom", "Hôte"...).
 */
function field(label: string): HTMLInputElement {
  const name = [...document.querySelectorAll('label')].find(
    (element) => element.textContent?.trim() === label,
  )
  const input = name === undefined ? null : document.getElementById(name.htmlFor)
  if (!(input instanceof HTMLInputElement)) {
    throw new Error(`no field ${label}`)
  }
  return input
}

/**
 * Types a value in a field, as the keyboard would.
 *
 * @param label Name of the setting.
 * @param value Value typed.
 */
async function type(label: string, value: string): Promise<void> {
  const input = field(label)
  input.value = value
  input.dispatchEvent(new Event('input'))
  await flushPromises()
}

/**
 * Clicks a button of the window.
 *
 * @param name Text or `aria-label` of the button.
 */
async function click(name: string): Promise<void> {
  button(name).click()
  await flushPromises()
}

/**
 * Shows a connection of the list, by its name.
 *
 * @param name Name of the connection.
 */
async function pick(name: string): Promise<void> {
  const line = [...document.querySelectorAll<HTMLButtonElement>('nav[aria-label] button')].find(
    (element) => element.querySelector('.font-medium')?.textContent?.trim() === name,
  )
  if (line === undefined) {
    throw new Error(`no connection ${name}`)
  }
  line.click()
  await flushPromises()
}

/**
 * Card of a kind of source, by its name.
 *
 * @param name Name of the kind ("PostgreSQL"...).
 */
function card(name: string): HTMLButtonElement {
  const found = [...document.querySelectorAll<HTMLButtonElement>('button[aria-pressed]')].find(
    (element) => element.textContent?.includes(name),
  )
  if (found === undefined) {
    throw new Error(`no card ${name}`)
  }
  return found
}

/**
 * Text of the list of the connections.
 */
function listText(): string {
  return document.querySelector('nav[aria-label="Connexions enregistrées"]')?.textContent ?? ''
}

beforeEach(() => {
  setActivePinia(createPinia())
  document.body.innerHTML = ''
  vi.clearAllMocks()
  saved = [connection(1, 'local'), connection(2, 'prod')]
  api.listConnections.mockImplementation(async () => saved.map((item) => ({ ...item })))
  api.probeConnection.mockResolvedValue(SERVER)
  api.probeSettings.mockResolvedValue(SERVER)
  api.updateConnection.mockResolvedValue()
  api.createConnection.mockImplementation(async (input) => {
    saved = [...saved, connection(3, input.name)]
    return { id: 3 }
  })
  api.copyConnection.mockImplementation(async (_, input) => {
    saved = [...saved, { ...connection(4, input.name), host: input.host }]
    return { id: 4 }
  })
})

describe('ConnectionManager', () => {
  it('keeps a changed connection as a draft until it is saved', async () => {
    await openManager(1)
    expect(field('Nom').value).toBe('local')
    expect(button('Enregistrer').disabled).toBe(true)

    await type('Hôte', 'autre.lan')
    expect(listText()).toContain('modifiée, non enregistrée')
    expect(document.body.textContent).toContain('Modifications non enregistrées')

    // Another connection shown, the draft waits, then comes back.
    await pick('prod')
    expect(field('Hôte').value).toBe('prod.lan')
    expect(document.body.textContent).toContain('1 connexion modifiée, non enregistrée')
    await pick('local')
    expect(field('Hôte').value).toBe('autre.lan')

    await click('Enregistrer')
    expect(api.updateConnection).toHaveBeenCalledWith(
      1,
      expect.objectContaining({ host: 'autre.lan', password: '' }),
    )
  })

  it('goes back to the saved settings', async () => {
    await openManager(2)
    await type('Port', '3307')
    await click('Annuler les modifications')
    expect(field('Port').value).toBe('3306')
    expect(listText()).not.toContain('modifiée')
  })

  it('creates a connection in three steps, its test shown before saving', async () => {
    await openManager('new')
    expect(document.body.textContent).toContain('Quel type de source ?')
    // The kinds not supported yet cannot be chosen.
    expect(card('PostgreSQL').disabled).toBe(true)
    expect(card('MySQL / MariaDB').getAttribute('aria-pressed')).toBe('true')

    await click('Suivant')
    await type('Nom', 'recette')
    expect(listText()).toContain('création en cours, étape 2 sur 3')

    await click('Tester')
    expect(document.body.textContent).toContain('Étape 3 sur 3')
    expect(document.body.textContent).toContain('Prisme a testé la connexion')

    // A failed test shows at the last step too, and does not prevent saving.
    await click('Retour')
    api.probeSettings.mockRejectedValueOnce(new Error('Connexion refusée'))
    await click('Tester')
    expect(document.body.textContent).toContain('Étape 3 sur 3')
    expect(document.body.textContent).toContain('Connexion refusée')
    expect(document.body.textContent).toContain('Le test a échoué')
    await click('Enregistrer')
    expect(api.createConnection).toHaveBeenCalledWith(expect.objectContaining({ name: 'recette' }))
    expect(field('Nom').value).toBe('recette')
    expect(listText()).not.toContain('création en cours')
  })

  it('copies a connection with the password of the copied one', async () => {
    await openManager(2)
    await click('Dupliquer')
    expect(field('Nom').value).toBe('prod-copie')

    await click('Tester')
    expect(api.probeSettings).toHaveBeenCalledWith(expect.objectContaining({ saved_id: 2 }))
    await click('Enregistrer')
    expect(api.copyConnection).toHaveBeenCalledWith(
      2,
      expect.objectContaining({ name: 'prod-copie', host: 'prod.lan', password: '' }),
    )
  })

  it('drops the drafts and the creation on closing, without asking', async () => {
    await openManager(1)
    await type('Nom', 'changé')
    await click('Nouvelle connexion')
    useConnectionsStore().closeManager()
    await flushPromises()

    useConnectionsStore().openManager(1)
    await flushPromises()
    expect(field('Nom').value).toBe('local')
    expect(listText()).not.toContain('création en cours')
  })
})
