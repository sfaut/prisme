import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'

import type { ExportInfo } from '@/api/generated/ExportInfo'
import type { ExportState } from '@/api/generated/ExportState'

import ExportItem from '../ExportItem.vue'
import ProgressDot from '../ProgressDot.vue'
import UiButton from '../ui/UiButton.vue'
import UiStatusBar from '../ui/UiStatusBar.vue'

beforeEach(() => setActivePinia(createPinia()))

/**
 * Export that wrote 2,000 rows of 2 MB in 4 s, ready unless `changes` say
 * otherwise.
 *
 * @param changes Fields that differ.
 */
function entry(changes: Partial<ExportInfo> = {}): ExportInfo {
  return {
    id: 1,
    name: 'requête-1-20261029-100000.csv',
    connection_name: 'local',
    sql: 'SELECT seq FROM seq_1_to_3000000',
    state: 'ready',
    rows: 2000,
    bytes: 2_000_000,
    expected_rows: null,
    created_at: '2026-10-29T10:00:00.000Z',
    ended_at: '2026-10-29T10:00:04.000Z',
    error: null,
    file_missing: false,
    ...changes,
  }
}

/**
 * Mounts an export in its list.
 *
 * @param shown The export.
 */
function mountItem(shown: ExportInfo) {
  return mount(ExportItem, { props: { entry: shown } })
}

/**
 * Labels of the badges of a mounted export: its pills.
 *
 * @param wrapper Mounted export.
 */
function badges(wrapper: ReturnType<typeof mountItem>): string[] {
  return wrapper.findAll('.rounded-full').map((badge) => badge.text())
}

/**
 * Labels of the buttons of a mounted export.
 *
 * @param wrapper Mounted export.
 */
function buttons(wrapper: ReturnType<typeof mountItem>): string[] {
  return wrapper
    .findAllComponents(UiButton)
    .map(
      (button) =>
        `${button.attributes('aria-label')}${button.props('disabled') ? ' (inactif)' : ''}`,
    )
}

describe('ExportItem', () => {
  it('shows the connection, the rows, the size, the duration and the throughput', () => {
    const wrapper = mountItem(entry())
    const items = wrapper.findComponent(UiStatusBar).findAll('span')
    expect(items.map((item) => item.text())).toEqual([
      'local',
      '2 000 lignes',
      '2,0 Mo',
      '4,0 s',
      '500,0 ko/s',
    ])
    expect(wrapper.text()).toContain('prêt')
    expect(buttons(wrapper)).toEqual(['Télécharger', 'Retirer'])
  })

  it('gives the rows and the duration of an interrupted export', () => {
    const wrapper = mountItem(entry({ state: 'interrupted' }))
    const items = wrapper.findComponent(UiStatusBar).findAll('span')
    expect(items.map((item) => item.text())).toEqual([
      'local',
      '2\u202f000 lignes',
      '2,0 Mo',
      '4,0 s',
      '500,0 ko/s',
    ])
    expect(wrapper.text()).toContain('interrompu')
  })

  it('offers the partial file of a stopped, failed or interrupted export, unless empty', () => {
    for (const state of ['cancelled', 'failed', 'interrupted'] as const) {
      expect(buttons(mountItem(entry({ state })))).toEqual(['Télécharger', 'Retirer'])
    }
    const empty = mountItem(entry({ state: 'failed', rows: 0, bytes: 0 }))
    expect(buttons(empty)).toEqual(['Télécharger (inactif)', 'Retirer'])
  })

  it('shows the completed share of a running export whose total is known', () => {
    const wrapper = mountItem(entry({ state: 'running', ended_at: null, expected_rows: 8000 }))
    expect(wrapper.findComponent(UiStatusBar).text()).toContain('2 000 lignes (25 %)')
    expect(buttons(wrapper)).toEqual(['Arrêter l’export', 'Retirer'])
  })

  it('shows its state as a badge', () => {
    const labels = {
      running: 'en cours',
      ready: 'prêt',
      failed: 'échoué',
      cancelled: 'arrêté',
      interrupted: 'interrompu',
    } satisfies Record<ExportState, string>
    for (const [state, label] of Object.entries(labels)) {
      expect(badges(mountItem(entry({ state: state as ExportState })))).toEqual([label])
    }
    // While it runs, with a progress dot.
    const running = mountItem(entry({ state: 'running' }))
    expect(running.findComponent(ProgressDot).exists()).toBe(true)
  })

  it('warns of a missing file, of rows Excel does not open, and of a failure', () => {
    const missing = mountItem(entry({ file_missing: true, rows: 1_048_576 }))
    expect(badges(missing)).toEqual(['prêt', 'fichier manquant'])
    expect(missing.text()).toContain('Excel n’en ouvre que les 1 048 575 premières lignes.')
    expect(buttons(missing)).toEqual(['Télécharger (inactif)', 'Retirer'])

    const failed = mountItem(entry({ state: 'failed', error: { code: 'export_not_read' } }))
    expect(failed.text()).toContain('échoué')
    expect(failed.text()).toContain('Export refusé : seule une lecture')
    expect(buttons(failed)).toEqual(['Télécharger', 'Retirer'])
  })
})
