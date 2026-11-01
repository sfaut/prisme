import { describe, expect, it } from 'vitest'

import { type PivotConfig, emptyConfig } from '../pivotConfig'
import { TabConfigs } from '../pivotTabs'

/**
 * Columns of the example of the specification.
 */
const COLUMNS = ['region', 'ville', 'annee', 'trimestre', 'montant']

/**
 * Configuration with one level of the rows, named.
 */
const CONFIG: PivotConfig = { ...emptyConfig(), rows: [{ column: 0, name: 'Région' }] }

describe('TabConfigs', () => {
  it('keeps the configuration of a tab while the columns of its result are the same', () => {
    const configs = new TabConfigs()
    configs.set(1, 1, COLUMNS, CONFIG)
    expect(configs.get(1, COLUMNS)).toBe(CONFIG)
    expect(configs.get(1, ['region'])).toBeNull()
    expect(configs.get(2, COLUMNS)).toBeNull()
  })

  it('carries the configuration of a removed tab over to the tab of its rank', () => {
    const configs = new TabConfigs()
    configs.set(1, 1, COLUMNS, CONFIG)
    configs.set(2, 2, ['x'], CONFIG)
    configs.keep([])
    expect(configs.get(1, COLUMNS)).toBeNull()
    // Tab 3 takes rank 2, with other columns: nothing to take back.
    configs.adopt(3, 2, COLUMNS)
    expect(configs.get(3, COLUMNS)).toBeNull()
    configs.adopt(4, 1, COLUMNS)
    expect(configs.get(4, COLUMNS)).toBe(CONFIG)
    // Taken back once.
    configs.adopt(5, 1, COLUMNS)
    expect(configs.get(5, COLUMNS)).toBeNull()
  })

  it('leaves a pinned tab and a tab with its own configuration as they are', () => {
    const configs = new TabConfigs()
    const own = emptyConfig()
    configs.set(1, 1, COLUMNS, CONFIG)
    configs.set(2, 1, COLUMNS, own)
    configs.keep([2])
    expect(configs.get(2, COLUMNS)).toBe(own)
    configs.adopt(2, 1, COLUMNS)
    expect(configs.get(2, COLUMNS)).toBe(own)
    configs.adopt(3, 1, COLUMNS)
    expect(configs.get(3, COLUMNS)).toBe(CONFIG)
  })
})
