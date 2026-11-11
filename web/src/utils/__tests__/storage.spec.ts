// TEMPORARY: tests of the migration of the keys (`utils/storage.ts`), to remove with it.
import { afterEach, describe, expect, it } from 'vitest'

import { migrateStorage } from '../storage'

afterEach(() => {
  window.localStorage.clear()
})

/**
 * Keys and values of the local storage, sorted by key.
 */
function kept(): Record<string, string> {
  return Object.fromEntries(
    Object.keys(window.localStorage)
      .sort()
      .map((key) => [key, window.localStorage.getItem(key)!]),
  )
}

describe('migrateStorage', () => {
  it('moves the keys of v0.11.0 under their new names, values unchanged', () => {
    window.localStorage.setItem('prisme.objectsOpen', 'false')
    window.localStorage.setItem('prisme.dataExplorerWrap', 'true')
    window.localStorage.setItem('prisme.consoleOpen', 'false')
    window.localStorage.setItem('prisme.consoleTab', 'debug')
    window.localStorage.setItem('prisme.consoleFiltersShown', 'true')
    window.localStorage.setItem('prisme.consoleHistoryFilter', '{"action":["run"]}')
    window.localStorage.setItem('prisme.consoleDebugFilter', '{"level":["error"]}')
    window.localStorage.setItem('prisme.activeEditor', '12')
    window.localStorage.setItem('prisme-color-mode', 'dark')
    migrateStorage()
    expect(kept()).toEqual({
      'prisme.app.colorMode': 'dark',
      'prisme.console.debugFilter': '{"level":["error"]}',
      'prisme.console.filtersShown': 'true',
      'prisme.console.historyFilter': '{"action":["run"]}',
      'prisme.console.open': 'false',
      'prisme.console.tab': 'debug',
      'prisme.editor.active': '12',
      'prisme.explorer.wrap': 'true',
      'prisme.navigator.open': 'false',
    })
  })

  it('removes the layouts of the splitter of Reka UI', () => {
    window.localStorage.setItem('reka:prisme.layout', '{}')
    window.localStorage.setItem('reka:prisme.centerLayout', '{}')
    window.localStorage.setItem('reka:prisme.cellLayout.1', '{}')
    migrateStorage()
    expect(kept()).toEqual({})
  })

  it('moves the layouts and the sizes of the development builds', () => {
    window.localStorage.setItem('prisme.cellLayout.3', '{"editor":129}')
    window.localStorage.setItem(
      'prisme.panelSizes',
      '{"navigator":227.67,"explorer":null,"console":-4}',
    )
    migrateStorage()
    expect(kept()).toEqual({
      'prisme.editor.layout.3': '{"editor":129}',
      'prisme.navigator.width': '227.67',
    })
  })

  it('keeps a value already under its new name, and the other keys', () => {
    window.localStorage.setItem('prisme.consoleTab', 'debug')
    window.localStorage.setItem('prisme.console.tab', 'history')
    window.localStorage.setItem('other', 'x')
    migrateStorage()
    expect(kept()).toEqual({ 'prisme.console.tab': 'history', other: 'x' })
  })
})
