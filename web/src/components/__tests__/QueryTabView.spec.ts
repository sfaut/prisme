import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

import { flushPromises, mount } from '@vue/test-utils'

import type { ExportInfo } from '@/api/generated/ExportInfo'
import type { ExportRequest } from '@/api/generated/ExportRequest'
import { type QueryTab, formatInstant } from '@/api/querySession'

import QueryTabView from '../QueryTabView.vue'
import ResultFilter from '../ResultFilter.vue'
import UiDropdownMenu from '../ui/UiDropdownMenu.vue'
import UiStatusBar from '../ui/UiStatusBar.vue'
import { finishedTab } from './resultTabs'

const startExport = vi.fn<(requested: ExportRequest) => Promise<ExportInfo>>()

vi.mock('@/api/exports', () => ({
  startExport: (requested: ExportRequest) => startExport(requested),
  listExports: async () => ({
    exports: [],
    orphans: { count: 0, bytes: 0 },
  }),
}))

beforeEach(() => {
  setActivePinia(createPinia())
  startExport.mockReset()
})

/**
 * Mounts the content of a tab, with its filter, on connection 7 and
 * database `shop`.
 *
 * @param tab Tab of the query.
 */
function mountTab(tab: QueryTab) {
  return mount(QueryTabView, {
    props: { tab, displayed: false, filter: tab.filter, connectionId: 7, database: 'shop' },
  })
}

/**
 * Runs the full export offered by the download menu of a mounted tab.
 *
 * @param wrapper Mounted tab.
 */
async function exportFull(wrapper: ReturnType<typeof mountTab>): Promise<void> {
  await nextTick()
  wrapper.findComponent(UiDropdownMenu).props('items')[1]?.onSelect?.()
  await flushPromises()
}

/**
 * Tab of a result with two columns `id` and `name`, three rows.
 */
function threeRows(): QueryTab {
  return finishedTab(
    ['id', 'name'],
    [
      ['1', 'a'],
      ['2', 'b'],
      ['3', 'c'],
    ],
  )
}

describe('QueryTabView', () => {
  it('shows the status of the result in its bar, one item each', async () => {
    const tab = finishedTab(['id'], [['1'], ['2']])
    const wrapper = mountTab(tab)
    const items = () =>
      wrapper
        .findComponent(UiStatusBar)
        .findAll('span')
        .map((item) => item.text())
    const instant = formatInstant(1_792_500_000_000)
    expect(items()).toEqual([
      '2 lignes',
      'exécution 2 ms',
      'transfert 3 ms',
      '120 o',
      '40,0 ko/s',
      instant,
    ])

    await wrapper.setProps({ filter: { text: '2', column: null } })
    expect(items()[0]).toBe('1 / 2 lignes')
  })

  it('shows the durations alone without a grid', () => {
    const wrapper = mountTab(finishedTab([]))
    const items = wrapper.findComponent(UiStatusBar).findAll('span')
    expect(items.map((item) => item.text())).toEqual(['exécution 3 ms'])
  })

  it('shows the form of the filter while it is expanded, and the grid under it', async () => {
    const tab = finishedTab(['id'], [['1']])
    const wrapper = mountTab(tab)
    expect(wrapper.findComponent(ResultFilter).exists()).toBe(false)
    expect(wrapper.find('.grid-scroller').classes()).toContain('attached')

    await wrapper.setProps({ tab: { ...tab, filterOpen: true } })
    expect(wrapper.findComponent(ResultFilter).exists()).toBe(true)
    expect(wrapper.find('.grid-scroller').classes()).not.toContain('attached')
  })

  it('offers to download the displayed result, or the selection when there is one', async () => {
    const wrapper = mountTab(
      finishedTab(
        ['id', 'name'],
        [
          ['1', 'a'],
          ['2', 'b'],
          ['3', 'c'],
        ],
      ),
    )
    const label = () => wrapper.findComponent(UiDropdownMenu).props('items')[0]?.label
    // The grid counts its rows once mounted: the menu follows on the next render.
    await nextTick()
    expect(label()).toBe('Résultat affiché en CSV (3 lignes)')

    await wrapper.find('.grid-corner').trigger('mousedown')
    expect(label()).toBe('Sélection en CSV (3 lignes × 2 colonnes)')
  })

  it('exports the full result, with its total when the displayed result is complete', async () => {
    startExport.mockRejectedValue(new Error('Serveur Prisme injoignable.'))
    await exportFull(mountTab(threeRows()))
    expect(startExport).toHaveBeenCalledWith({
      connection_id: 7,
      database: 'shop',
      sql: 'SELECT 1',
      columns: [0, 1],
      name: expect.stringMatching(/^requête-1-\d{8}-\d{6}\.csv$/),
      expected_rows: 3,
    })

    const truncated = threeRows()
    truncated.result = { ...truncated.result!, truncated: true }
    await exportFull(mountTab(truncated))
    expect(startExport.mock.calls[1]?.[0].expected_rows).toBeUndefined()
  })

  it('refuses the full export of columns of the same name', async () => {
    await exportFull(mountTab(finishedTab(['id', 'id'], [['1', '2']])))
    expect(startExport).not.toHaveBeenCalled()
  })

  it('has no download without a grid', () => {
    const wrapper = mountTab(finishedTab([]))
    expect(wrapper.findComponent(UiDropdownMenu).exists()).toBe(false)
  })

  it('makes a targeted filter global for new columns, and keeps it for the same ones', async () => {
    const tab = { ...finishedTab(['id'], [['1']]), filter: { text: 'a', column: 0 } }
    const wrapper = mountTab(tab)

    await wrapper.setProps({ tab: { ...tab, result: finishedTab(['id'], [['2']]).result } })
    expect(wrapper.emitted('update:filter')).toBeUndefined()

    await wrapper.setProps({ tab: { ...tab, result: finishedTab(['name'], [['2']]).result } })
    expect(wrapper.emitted('update:filter')).toEqual([[{ text: 'a', column: null }]])
  })
})
