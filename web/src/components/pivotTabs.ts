/**
 * Configurations of the Pivoteur by result tab (`PivotPanel.vue`, F-04): each tab keeps its own,
 * made for the columns of its result and forgotten when they change.
 *
 * A new run of the queries replaces the tabs of the cell, except the pinned ones: the
 * configuration of a removed tab waits for the tab that takes its rank, the number of its query
 * in its batch, which takes it back when the columns of its result are the same.
 */

import { shallowReactive } from 'vue'

import type { PivotConfig } from './pivotConfig'
import { sameColumns } from './resultGrid'

/**
 * Configuration of a tab, with what it was made for.
 */
interface TabConfig {
  /**
   * Rank of the tab: number of its query in its batch.
   */
  rank: number
  /**
   * Names of the columns of the result, when the configuration was made.
   */
  columns: string[]
  /**
   * Configuration.
   */
  config: PivotConfig
}

/**
 * Configurations of the tabs of a cell.
 */
export class TabConfigs {
  /**
   * Configuration of each tab, by id. Shallow: a configuration is replaced as a whole, never
   * changed in place, and stays the plain object given.
   */
  private readonly byTab = shallowReactive(new Map<number, TabConfig>())

  /**
   * Configurations of the removed tabs, by rank, until a tab takes their rank.
   */
  private readonly byRank = new Map<number, TabConfig>()

  /**
   * Configuration of a tab, `null` without one or when the columns of its result have changed.
   *
   * @param tab Id of the tab.
   * @param columns Names of the columns of its result.
   */
  get(tab: number, columns: string[]): PivotConfig | null {
    const entry = this.byTab.get(tab)
    return entry !== undefined && sameColumns(entry.columns, columns) ? entry.config : null
  }

  /**
   * Records the configuration of a tab.
   *
   * @param tab Id of the tab.
   * @param rank Rank of the tab.
   * @param columns Names of the columns of its result.
   * @param config Configuration.
   */
  set(tab: number, rank: number, columns: string[], config: PivotConfig): void {
    this.byTab.set(tab, { rank, columns, config })
  }

  /**
   * Keeps the configurations of the tabs still there; that of a removed tab waits for the tab
   * that takes its rank.
   *
   * @param ids Ids of the tabs of the cell.
   */
  keep(ids: number[]): void {
    for (const [id, entry] of this.byTab) {
      if (!ids.includes(id)) {
        this.byRank.set(entry.rank, entry)
        this.byTab.delete(id)
      }
    }
  }

  /**
   * Gives a tab without configuration that of the removed tab of its rank, when it was made for
   * the same columns.
   *
   * @param tab Id of the tab.
   * @param rank Rank of the tab.
   * @param columns Names of the columns of its result.
   */
  adopt(tab: number, rank: number, columns: string[]): void {
    const entry = this.byRank.get(rank)
    if (!this.byTab.has(tab) && entry !== undefined && sameColumns(entry.columns, columns)) {
      this.byTab.set(tab, entry)
      this.byRank.delete(rank)
    }
  }
}
