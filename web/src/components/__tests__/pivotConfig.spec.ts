import { describe, expect, it } from 'vitest'
import { reactive } from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { Cell } from '@/api/querySession'

import {
  type DatePrecision,
  type NamedMeasure,
  type PivotColumnFilter,
  type PivotConfig,
  type PivotFilter,
  type PivotLevel,
  type PivotMeasureFilter,
  activeFilterPlace,
  addsNumbers,
  columnAt,
  defaultAggregate,
  elementExpression,
  elementLabel,
  emptyConfig,
  engineLayout,
  engineOrder,
  engineRequest,
  engineTargets,
  fittedConfig,
  hasGrandTotal,
  hasTable,
  isEmptyConfig,
  isRefused,
  measureExpression,
  operandExample,
  operandRefusal,
  placeColumn,
  precisionChoices,
  removeElement,
  sameOrder,
  sameRequest,
  sameTableLayout,
  sameTarget,
  withAggregate,
  withAllSubtotals,
  withColumnWidth,
  withFilter,
  withForm,
  withGrandTotal,
  withLabelWidth,
  withLevelSort,
  withName,
  withPrecision,
  withRepeatedLabels,
  withSubtotal,
  withValueSort,
  zoneLabels,
} from '../pivotConfig'

/**
 * Columns of the example of the specification.
 */
const NAMES = ['region', 'ville', 'annee', 'trimestre', 'montant']

/**
 * Level of a column, with its name.
 *
 * @param column Index of the column.
 * @param name Name given, none by default.
 * @param precision What it keeps of its dates, its values as they are by default.
 */
function level(column: number, name = '', precision: DatePrecision = 'none'): PivotLevel {
  return { column, name, precision }
}

/**
 * Measure of a column, with its name.
 *
 * @param aggregate Aggregate.
 * @param column Index of the column.
 * @param name Name given, none by default.
 */
function measure(aggregate: NamedMeasure['aggregate'], column: number, name = ''): NamedMeasure {
  return { aggregate, column, name }
}

describe('defaultAggregate', () => {
  const origin = (primary_key: boolean) => ({ schema: 's', table: 't', column: 'c', primary_key })

  it('sums a number and counts the other columns', () => {
    expect(defaultAggregate({ family: 'decimal', name: 'DECIMAL(10,2)' }, false)).toBe('sum')
    expect(defaultAggregate({ family: 'integer', name: 'INT' }, false)).toBe('sum')
    expect(defaultAggregate({ family: 'float', name: 'DOUBLE' }, false)).toBe('sum')
    expect(defaultAggregate({ family: 'text', name: 'TEXT' }, false)).toBe('count')
    expect(defaultAggregate(undefined, false)).toBe('count')
    expect(defaultAggregate({ family: 'integer', name: 'INT', origin: origin(false) }, false)).toBe(
      'sum',
    )
  })

  it('counts the rows for a primary key, whatever its type', () => {
    expect(defaultAggregate({ family: 'integer', name: 'INT', origin: origin(true) }, false)).toBe(
      'count_star',
    )
    expect(defaultAggregate({ family: 'text', name: 'CHAR(2)', origin: origin(true) }, false)).toBe(
      'count_star',
    )
  })

  it('counts the values of a foreign key, unless it is part of the primary key too', () => {
    expect(defaultAggregate({ family: 'integer', name: 'INT', origin: origin(false) }, true)).toBe(
      'count',
    )
    expect(defaultAggregate({ family: 'integer', name: 'INT', origin: origin(true) }, true)).toBe(
      'count_star',
    )
  })
})

describe('measureExpression', () => {
  it('writes the expression of the measure', () => {
    expect(measureExpression(measure('sum', 4), NAMES)).toBe('SUM(montant)')
    expect(measureExpression(measure('count', 1), NAMES)).toBe('COUNT(ville)')
    expect(measureExpression(measure('count_distinct', 1), NAMES)).toBe('COUNT(DISTINCT ville)')
    expect(measureExpression(measure('count_star', 1), NAMES)).toBe('COUNT(*)')
    expect(measureExpression(measure('sum_distinct', 4), NAMES)).toBe('SUM(DISTINCT montant)')
    expect(measureExpression(measure('avg', 4), NAMES)).toBe('AVG(montant)')
    expect(measureExpression(measure('avg_distinct', 4), NAMES)).toBe('AVG(DISTINCT montant)')
    expect(measureExpression(measure('min', 1), NAMES)).toBe('MIN(ville)')
    expect(measureExpression(measure('max', 1), NAMES)).toBe('MAX(ville)')
  })
})

describe('addsNumbers', () => {
  it('asks for numbers only for the aggregates that add them up', () => {
    const adding = ['sum', 'sum_distinct', 'avg', 'avg_distinct'] as const
    const others = ['count_star', 'count', 'count_distinct', 'min', 'max'] as const
    expect(adding.every(addsNumbers)).toBe(true)
    expect(others.some(addsNumbers)).toBe(false)
  })
})

describe('engineRequest', () => {
  it('asks the engine for the columns and the aggregates, without the names', () => {
    const config: PivotConfig = {
      rows: [level(0, 'Région')],
      columns: [level(2)],
      measures: [measure('sum', 4, 'CA')],
      filters: [],
    }
    const request = engineRequest(config)
    expect(request).toEqual({
      rows: [{ column: 0, precision: 'none' }],
      columns: [{ column: 2, precision: 'none' }],
      measures: [{ aggregate: 'sum', column: 4 }],
      filters: [],
    })
    expect(
      sameRequest(request, engineRequest(withName(config, { zone: 'rows', index: 0 }, 'R', NAMES))),
    ).toBe(true)
    expect(sameRequest(request, engineRequest(withAggregate(config, 0, 'count')))).toBe(false)
  })

  it('asks for another table when a precision changes', () => {
    const config: PivotConfig = { rows: [level(0)], columns: [], measures: [], filters: [] }
    const changed = withPrecision(config, { zone: 'rows', index: 0 }, 'month')
    expect(engineRequest(changed).rows).toEqual([{ column: 0, precision: 'month' }])
    expect(sameRequest(engineRequest(config), engineRequest(changed))).toBe(false)
  })
})

describe('precisionChoices', () => {
  const choices = (family: Parameters<typeof precisionChoices>[0]) =>
    precisionChoices(family).map(({ precision, label }) => [precision, label])

  it('offers the periods of a date, a date being its own day', () => {
    expect(choices('date')).toEqual([
      ['year', 'Année'],
      ['quarter', 'Trimestre'],
      ['month', 'Mois'],
      ['week', 'Semaine'],
      ['none', 'Date'],
    ])
  })

  it('offers the day of a date and time, then its values as they are', () => {
    expect(choices('datetime').slice(-2)).toEqual([
      ['date', 'Date'],
      ['none', 'Date et heure'],
    ])
  })

  it('offers nothing for a column that holds no dates', () => {
    for (const family of ['time', 'year', 'text', 'integer', undefined] as const) {
      expect(choices(family)).toEqual([])
    }
  })
})

describe('withPrecision', () => {
  const config: PivotConfig = {
    rows: [level(0), level(1, 'Ville')],
    columns: [level(2)],
    measures: [measure('count', 3)],
    filters: [],
  }

  it('changes the precision of a level only, which keeps its name', () => {
    const changed = withPrecision(config, { zone: 'rows', index: 1 }, 'week')
    expect(changed.rows).toEqual([level(0), level(1, 'Ville', 'week')])
    expect(changed.columns).toBe(config.columns)
    expect(changed.measures).toBe(config.measures)
    expect(withPrecision(config, { zone: 'columns', index: 0 }, 'year').columns).toEqual([
      level(2, '', 'year'),
    ])
  })

  it('shows the precision in the expression of the level, not over its name', () => {
    const changed: PivotConfig = {
      rows: [level(0, '', 'month'), level(1, 'Ville', 'week'), level(2, '', 'date')],
      columns: [level(3, '', 'none')],
      measures: [],
      filters: [],
    }
    expect(zoneLabels(changed, 'rows', NAMES)).toEqual(['region (Mois)', 'Ville', 'annee (Date)'])
    expect(zoneLabels(changed, 'columns', NAMES)).toEqual(['trimestre'])
  })

  it('removes a name equal to the expression with its precision', () => {
    const changed = { ...config, rows: [level(0, '', 'month')] }
    const named = withName(changed, { zone: 'rows', index: 0 }, 'region (Mois)', NAMES)
    expect(named.rows[0]).toEqual(level(0, '', 'month'))
  })
})

describe('elementLabel', () => {
  it('shows the name of an element, or its expression without one', () => {
    const config: PivotConfig = {
      rows: [level(0, 'Région'), level(1)],
      columns: [],
      measures: [measure('sum', 4, 'CA'), measure('count_star', 0)],
      filters: [],
    }
    expect(zoneLabels(config, 'rows', NAMES)).toEqual(['Région', 'ville'])
    expect(zoneLabels(config, 'measures', NAMES)).toEqual(['CA', 'COUNT(*)'])
    expect(elementLabel(config, { zone: 'columns', index: 0 }, NAMES)).toBe('?')
  })
})

describe('withName', () => {
  it('names an element, and an empty name or its expression removes the name', () => {
    const config: PivotConfig = {
      rows: [level(0)],
      columns: [level(2)],
      measures: [measure('sum', 4)],
      filters: [],
    }
    const named = withName(config, { zone: 'measures', index: 0 }, '  CA ', NAMES)
    expect(named.measures).toEqual([measure('sum', 4, 'CA')])
    expect(named.rows).toBe(config.rows)
    expect(withName(named, { zone: 'measures', index: 0 }, ' ', NAMES).measures).toEqual([
      measure('sum', 4),
    ])
    expect(withName(config, { zone: 'columns', index: 0 }, 'Année', NAMES).columns).toEqual([
      level(2, 'Année'),
    ])
    expect(withName(config, { zone: 'rows', index: 0 }, 'region', NAMES).rows).toEqual([level(0)])
  })
})

describe('placeColumn', () => {
  it('adds a level or a measure at a position', () => {
    let config = placeColumn(emptyConfig(), 0, 'count', { zone: 'rows', index: 0 }, null)
    config = placeColumn(config, 1, 'count', { zone: 'rows', index: 0 }, null)
    config = placeColumn(config, 4, 'sum', { zone: 'measures', index: 9 }, null)
    expect(config).toEqual({
      rows: [level(1), level(0)],
      columns: [],
      measures: [measure('sum', 4)],
      filters: [],
    })
    expect(isEmptyConfig(config)).toBe(false)
    expect(isEmptyConfig(emptyConfig())).toBe(true)
  })

  it('adds a new level for a column of the list, even one already placed', () => {
    const config: PivotConfig = {
      rows: [level(0, 'Région'), level(1)],
      columns: [level(2)],
      measures: [],
      filters: [],
    }
    expect(placeColumn(config, 0, 'count', { zone: 'columns', index: 0 }, null)).toEqual({
      rows: [level(0, 'Région'), level(1)],
      columns: [level(0), level(2)],
      measures: [],
      filters: [],
    })
    expect(placeColumn(config, 0, 'count', { zone: 'rows', index: 2 }, null).rows).toEqual([
      level(0, 'Région'),
      level(1),
      level(0),
    ])
  })

  it('moves one level of a column placed twice, with its name and its precision', () => {
    const config: PivotConfig = {
      rows: [level(0, 'An', 'year'), level(0, 'Mois', 'month')],
      columns: [],
      measures: [],
      filters: [],
    }
    const moved = placeColumn(
      config,
      0,
      'count',
      { zone: 'columns', index: 0 },
      { zone: 'rows', index: 0 },
    )
    expect(moved.rows).toEqual([level(0, 'Mois', 'month')])
    expect(moved.columns).toEqual([level(0, 'An', 'year')])
  })

  it('moves an element within its zone, the target counted before the move', () => {
    const config: PivotConfig = {
      rows: [level(0), level(1), level(2)],
      columns: [],
      measures: [],
      filters: [],
    }
    const from = { zone: 'rows' as const, index: 0 }
    const columns = (moved: PivotConfig) => moved.rows.map((item) => item.column)
    expect(columns(placeColumn(config, 0, 'count', { zone: 'rows', index: 2 }, from))).toEqual([
      1, 0, 2,
    ])
    expect(columns(placeColumn(config, 0, 'count', { zone: 'rows', index: 3 }, from))).toEqual([
      1, 2, 0,
    ])
    expect(
      columns(
        placeColumn(config, 2, 'count', { zone: 'rows', index: 0 }, { zone: 'rows', index: 2 }),
      ),
    ).toEqual([2, 0, 1])
  })

  it('keeps a moved measure, and gives its default measure to a level that becomes one', () => {
    const config: PivotConfig = {
      rows: [level(0, 'Région')],
      columns: [],
      measures: [measure('count_star', 1, 'Lignes'), measure('sum', 4, 'CA')],
      filters: [],
    }
    const moved = placeColumn(
      config,
      1,
      'count',
      { zone: 'measures', index: 2 },
      { zone: 'measures', index: 0 },
    )
    expect(moved.measures).toEqual([measure('sum', 4, 'CA'), measure('count_star', 1, 'Lignes')])
    const fromLevel = placeColumn(
      config,
      0,
      'count',
      { zone: 'measures', index: 0 },
      { zone: 'rows', index: 0 },
    )
    expect(fromLevel).toEqual({
      rows: [],
      columns: [],
      measures: [measure('count', 0), ...config.measures],
      filters: [],
    })
    const fromMeasure = placeColumn(
      config,
      4,
      'sum',
      { zone: 'columns', index: 0 },
      { zone: 'measures', index: 1 },
    )
    expect(fromMeasure).toEqual({
      rows: [level(0, 'Région')],
      columns: [level(4)],
      measures: [measure('count_star', 1, 'Lignes')],
      filters: [],
    })
  })
})

describe('removeElement', () => {
  it('removes one element of one zone', () => {
    const config: PivotConfig = {
      rows: [level(0), level(1)],
      columns: [level(2)],
      measures: [measure('sum', 4)],
      filters: [],
    }
    expect(removeElement(config, { zone: 'rows', index: 0 })).toEqual({
      rows: [level(1)],
      columns: [level(2)],
      measures: config.measures,
      filters: [],
    })
    expect(removeElement(config, { zone: 'measures', index: 0 }).measures).toEqual([])
    expect(columnAt(config, { zone: 'columns', index: 0 })).toBe(2)
    expect(columnAt(config, { zone: 'measures', index: 0 })).toBe(4)
  })
})

describe('fittedConfig', () => {
  const config: PivotConfig = {
    rows: [level(0, 'Région')],
    columns: [level(1)],
    measures: [measure('sum', 4, 'Total'), measure('count_star', 2)],
    filters: [],
  }

  it('gives the configuration itself when the columns are the same', () => {
    expect(fittedConfig({ fields: NAMES, config }, [...NAMES], [])).toBe(config)
  })

  it('follows the columns by name', () => {
    const moved = ['montant', 'annee', 'region', 'ville', 'trimestre']
    expect(fittedConfig({ fields: NAMES, config }, moved, [])).toEqual({
      rows: [level(2, 'Région')],
      columns: [level(3)],
      measures: [measure('sum', 0, 'Total'), measure('count_star', 1)],
      filters: [],
    })
  })

  it('removes the elements of a lost column, except COUNT(*), which reads none', () => {
    expect(fittedConfig({ fields: NAMES, config }, ['ville', 'trimestre'], [])).toEqual({
      rows: [],
      columns: [level(0)],
      measures: [measure('count_star', 0)],
      filters: [],
    })
  })

  it('takes the n-th column of a repeated name for the n-th', () => {
    const fields = ['id', 'id', 'x']
    const repeated: PivotConfig = {
      rows: [level(1)],
      columns: [],
      measures: [measure('count', 0)],
      filters: [],
    }
    expect(fittedConfig({ fields, config: repeated }, ['x', 'id', 'id'], [])).toEqual({
      rows: [level(2)],
      columns: [],
      measures: [measure('count', 1)],
      filters: [],
    })
    expect(fittedConfig({ fields, config: repeated }, ['id'], []).rows).toEqual([])
  })

  it('keeps nothing to fit when the configuration is empty', () => {
    const empty = emptyConfig()
    expect(fittedConfig({ fields: [], config: empty }, NAMES, [])).toBe(empty)
  })

  describe('with the types of the columns', () => {
    const fields = ['d', 'x']
    const dated: PivotConfig = {
      rows: [level(0, 'Jour', 'date')],
      columns: [level(1)],
      measures: [],
      filters: [],
    }
    const typed = (family: 'date' | 'datetime' | 'text') => [
      { family, name: 'T' },
      { family: 'text' as const, name: 'TEXT' },
    ]

    it('keeps a precision the column can take', () => {
      expect(fittedConfig({ fields, config: dated }, fields, typed('datetime'))).toBe(dated)
    })

    it('keeps a precision while the column has no type', () => {
      expect(fittedConfig({ fields, config: dated }, fields, [])).toBe(dated)
    })

    it('drops a precision the column can no longer take, with its name kept', () => {
      for (const family of ['text', 'date'] as const) {
        expect(fittedConfig({ fields, config: dated }, fields, typed(family)).rows).toEqual([
          level(0, 'Jour'),
        ])
      }
    })

    it('follows a column by name with its precision', () => {
      expect(
        fittedConfig(
          { fields, config: dated },
          ['x', 'd'],
          [
            { family: 'text', name: 'TEXT' },
            { family: 'datetime', name: 'T' },
          ],
        ).rows,
      ).toEqual([level(1, 'Jour', 'date')])
    })
  })
})

describe('withAggregate', () => {
  it('changes the aggregate of one measure, which keeps its name', () => {
    const config: PivotConfig = {
      rows: [],
      columns: [],
      measures: [measure('sum', 4), measure('sum', 2, 'Années')],
      filters: [],
    }
    expect(withAggregate(config, 1, 'count').measures).toEqual([
      measure('sum', 4),
      measure('count', 2, 'Années'),
    ])
  })
})

describe('the sorts', () => {
  /**
   * Configuration of the example: lines by the column 2025 on the second measure, columns by the
   * line Nord.
   */
  const SORTED: PivotConfig = {
    rows: [level(0), level(1)],
    columns: [level(2)],
    measures: [measure('sum', 4), measure('count', 4)],
    filters: [],
    sort_lines: { path: ['2025'], measure: 1, descending: true },
    sort_columns: { path: ['Nord'], measure: 0, descending: false },
  }

  describe('engineOrder', () => {
    it('reads the labels in ascending order without any sort', () => {
      const config: PivotConfig = {
        rows: [level(0)],
        columns: [level(2)],
        measures: [],
        filters: [],
      }
      expect(engineOrder(config)).toEqual({
        rows: { descending: [], values: null },
        columns: { descending: [], values: null },
      })
    })

    it('gives the direction of each level and the values that rank each axis', () => {
      const config: PivotConfig = {
        ...SORTED,
        rows: [level(0), { ...level(1), descending: true }],
      }
      expect(engineOrder(config)).toEqual({
        rows: {
          descending: [false, true],
          values: { path: ['2025'], measure: 1, descending: true },
        },
        columns: {
          descending: [],
          values: { path: ['Nord'], measure: 0, descending: false },
        },
      })
    })

    it('tells a level put back in ascending order from nothing', () => {
      const config: PivotConfig = { rows: [level(0)], columns: [], measures: [], filters: [] }
      const back = withLevelSort(withLevelSort(config, 'rows', 0), 'rows', 0)
      expect(sameOrder(engineOrder(back), engineOrder(config))).toBe(true)
    })

    it('asks for another layout when a sort changes, and for the same table', () => {
      const sorted = withLevelSort(SORTED, 'rows', 0)
      expect(sameOrder(engineOrder(sorted), engineOrder(SORTED))).toBe(false)
      expect(sameRequest(engineRequest(sorted), engineRequest(SORTED))).toBe(true)
    })
  })

  describe('withLevelSort', () => {
    it('inverts the direction of one level', () => {
      const config: PivotConfig = {
        rows: [level(0), level(1)],
        columns: [],
        measures: [],
        filters: [],
      }
      const once = withLevelSort(config, 'rows', 1)
      expect(once.rows).toEqual([level(0), { ...level(1), descending: true }])
      expect(withLevelSort(once, 'rows', 1).rows[1]?.descending).toBe(false)
      expect(withLevelSort(config, 'columns', 0)).toEqual(config)
    })

    it('gives an axis ranked by values back to its labels, in their direction', () => {
      const descending: PivotConfig = {
        ...SORTED,
        rows: [{ ...level(0), descending: true }, level(1)],
      }
      const labels = withLevelSort(descending, 'rows', 1)
      expect(labels.sort_lines).toBeUndefined()
      expect(labels.rows).toEqual(descending.rows)
      expect(labels.sort_columns).toEqual(SORTED.sort_columns)
    })
  })

  describe('withValueSort', () => {
    it('goes through ascending, descending, then none, as in the grid', () => {
      const config: PivotConfig = {
        rows: [level(0)],
        columns: [level(2)],
        measures: [],
        filters: [],
      }
      const target = { path: ['2025', null], measure: 1 }
      const ascending = withValueSort(config, 'rows', target)
      expect(ascending.sort_lines).toEqual({ ...target, descending: false })
      const descending = withValueSort(ascending, 'rows', target)
      expect(descending.sort_lines).toEqual({ ...target, descending: true })
      expect(withValueSort(descending, 'rows', target).sort_lines).toBeUndefined()
    })

    it('starts in ascending order on another node or another measure', () => {
      const other = withValueSort(SORTED, 'rows', { path: ['2026'], measure: 1 })
      expect(other.sort_lines).toEqual({ path: ['2026'], measure: 1, descending: false })
      const measureOf = withValueSort(SORTED, 'rows', { path: ['2025'], measure: 0 })
      expect(measureOf.sort_lines).toEqual({ path: ['2025'], measure: 0, descending: false })
      expect(other.sort_columns).toEqual(SORTED.sort_columns)
    })

    it('sorts the columns apart from the lines', () => {
      const none = withValueSort(SORTED, 'columns', { path: ['Sud'], measure: 0 })
      expect(none.sort_columns).toEqual({ path: ['Sud'], measure: 0, descending: false })
      expect(none.sort_lines).toEqual(SORTED.sort_lines)
    })
  })

  describe('sameTarget', () => {
    it('compares the values of the path, NULL apart from the text, and the measure', () => {
      const target = { path: ['a', null], measure: 0 }
      expect(sameTarget(target, { path: ['a', null], measure: 0 })).toBe(true)
      expect(sameTarget(target, { path: ['a', 'NULL'], measure: 0 })).toBe(false)
      expect(sameTarget(target, { path: ['a'], measure: 0 })).toBe(false)
      expect(sameTarget(target, { path: ['a', null], measure: 1 })).toBe(false)
    })
  })

  describe('a change of the zones', () => {
    it('drops the sort of the lines with a change of the levels of the columns', () => {
      const added = placeColumn(SORTED, 3, 'sum', { zone: 'columns', index: 1 }, null)
      expect(added.sort_lines).toBeUndefined()
      expect(added.sort_columns).toEqual(SORTED.sort_columns)
      const removed = removeElement(SORTED, { zone: 'columns', index: 0 })
      expect(removed.sort_lines).toBeUndefined()
    })

    it('drops the sort of the columns with a change of the levels of the rows', () => {
      const removed = removeElement(SORTED, { zone: 'rows', index: 1 })
      expect(removed.sort_columns).toBeUndefined()
      expect(removed.sort_lines).toEqual(SORTED.sort_lines)
    })

    it('drops the sort of the zone whose precision changes', () => {
      const place = { zone: 'columns', index: 0 } as const
      const month = withPrecision(SORTED, place, 'month')
      expect(month.sort_lines).toBeUndefined()
      expect(month.sort_columns).toEqual(SORTED.sort_columns)
    })

    it('keeps the sorts when an element changes its name', () => {
      const named = withName(SORTED, { zone: 'rows', index: 0 }, 'Région', NAMES)
      expect(named.sort_lines).toEqual(SORTED.sort_lines)
      expect(named.sort_columns).toEqual(SORTED.sort_columns)
    })

    it('follows the measure that ranks the lines, and drops the sort without it', () => {
      const moved = placeColumn(
        SORTED,
        4,
        'sum',
        { zone: 'measures', index: 0 },
        { zone: 'measures', index: 1 },
      )
      expect(moved.measures).toEqual([measure('count', 4), measure('sum', 4)])
      expect(moved.sort_lines?.measure).toBe(0)
      const first = removeElement(SORTED, { zone: 'measures', index: 0 })
      expect(first.sort_lines?.measure).toBe(0)
      const ranking = removeElement(SORTED, { zone: 'measures', index: 1 })
      expect(ranking.sort_lines).toBeUndefined()
      expect(ranking.sort_columns).toEqual(SORTED.sort_columns)
    })

    it('drops the sort of the columns with the last measure', () => {
      const one: PivotConfig = {
        ...SORTED,
        measures: [measure('sum', 4)],
        filters: [],
        sort_lines: undefined,
      }
      expect(removeElement(one, { zone: 'measures', index: 0 }).sort_columns).toBeUndefined()
    })

    it('keeps the direction of a level that moves to the other axis', () => {
      const config: PivotConfig = {
        rows: [{ ...level(0), descending: true }],
        columns: [],
        measures: [],
        filters: [],
      }
      const moved = placeColumn(
        config,
        0,
        'sum',
        { zone: 'columns', index: 0 },
        { zone: 'rows', index: 0 },
      )
      expect(moved.columns[0]?.descending).toBe(true)
    })
  })

  describe('fittedConfig', () => {
    it('keeps the sorts of the columns that follow their names', () => {
      const moved = ['montant', 'region', 'ville', 'annee', 'trimestre']
      const fitted = fittedConfig({ fields: NAMES, config: SORTED }, moved, [])
      expect(fitted.rows).toEqual([level(1), level(2)])
      expect(fitted.sort_lines).toEqual(SORTED.sort_lines)
      expect(fitted.sort_columns).toEqual(SORTED.sort_columns)
    })

    it('drops the sorts with an element lost', () => {
      const lost = ['region', 'annee', 'trimestre', 'montant']
      const fitted = fittedConfig({ fields: NAMES, config: SORTED }, lost, [])
      expect(fitted.rows).toEqual([level(0)])
      expect(fitted.sort_lines).toBeUndefined()
      expect(fitted.sort_columns).toBeUndefined()
    })
  })
})

describe('the filters', () => {
  /**
   * Filter on the values of a column.
   *
   * @param column Index of the column.
   * @param values Values kept.
   */
  function values(column: number, kept: Cell[]): PivotFilter {
    return tested({ column, condition: 'in_list', values: kept })
  }

  /**
   * Filter on a column of dates, without a value.
   *
   * @param changes What differs from it.
   */
  function tested(changes: Partial<PivotColumnFilter> = {}): PivotColumnFilter {
    return {
      kind: 'column',
      column: 2,
      name: '',
      precision: 'none',
      condition: 'between',
      values: [],
      operand: '',
      upper: '',
      ...changes,
    }
  }

  /**
   * Filter `SUM(montant) > 100`.
   *
   * @param changes What differs from it.
   */
  function above(changes: Partial<PivotMeasureFilter> = {}): PivotMeasureFilter {
    return {
      kind: 'measure',
      aggregate: 'sum',
      column: 4,
      name: '',
      condition: 'greater',
      operand: '100',
      upper: '',
      level_precision: 'none',
      ...changes,
    }
  }

  /**
   * Configuration of the filters given, on the levels of the example.
   *
   * @param filters Filters.
   */
  function filtered(...filters: PivotFilter[]): PivotConfig {
    return {
      rows: [level(0), level(1)],
      columns: [level(2, '', 'none')],
      measures: [measure('sum', 4)],
      filters,
    }
  }

  /**
   * Types of the given families, one per column.
   *
   * @param families Families.
   */
  function types(...families: ColumnTypeInfo['family'][]): ColumnTypeInfo[] {
    return families.map((family) => ({ family, name: family }))
  }

  describe('engineRequest', () => {
    it('asks only for the filters that have an effect', () => {
      const request = engineRequest(
        filtered(values(0, []), values(1, ['Lille', null]), above({ operand: '' })),
      )
      expect(request.filters).toEqual([
        {
          kind: 'values',
          level: { column: 1, precision: 'none' },
          values: ['Lille', null],
          excluded: false,
        },
      ])
    })

    it('asks a test for all its values', () => {
      const active = (changes: Partial<PivotColumnFilter>) =>
        engineRequest(filtered(tested(changes))).filters.length
      expect(active({})).toBe(0)
      expect(active({ operand: '1' })).toBe(0)
      expect(active({ upper: '2' })).toBe(0)
      expect(active({ operand: '1', upper: '2' })).toBe(1)
      expect(active({ condition: 'not_between', operand: '1' })).toBe(0)
      expect(active({ condition: 'less', operand: '1' })).toBe(1)
      expect(active({ condition: 'is_null' })).toBe(1)
      expect(active({ condition: 'is_not_null' })).toBe(1)
      const measures = (changes: Partial<PivotMeasureFilter>) =>
        engineRequest(filtered(above(changes))).filters.length
      expect(measures({ condition: 'between' })).toBe(0)
      expect(measures({ condition: 'between', upper: '400' })).toBe(1)
      expect(measures({ condition: 'is_null', operand: '' })).toBe(1)
    })

    it('sends a test at the precision of its filter', () => {
      const condition = 'between'
      const [filter] = engineRequest(
        filtered(tested({ condition, precision: 'month', operand: '2026-01', upper: '2026-03' })),
      ).filters
      expect(filter).toEqual({
        kind: 'column',
        level: { column: 2, precision: 'month' },
        test: { condition, operand: '2026-01', upper: '2026-03' },
      })
    })

    it('sends a list at the precision of its filter, and NOT IN without NULL', () => {
      const list = { precision: 'month', values: ['2026-01', null] } as const
      const request = (condition: PivotColumnFilter['condition']) =>
        engineRequest(filtered(tested({ ...list, values: [...list.values], condition }))).filters
      const level = { column: 2, precision: 'month' }
      expect(request('in_list')).toEqual([
        { kind: 'values', level, values: ['2026-01', null], excluded: false },
      ])
      expect(request('not_in_list')).toEqual([
        { kind: 'values', level, values: ['2026-01'], excluded: true },
      ])
      // NULL alone leaves no value to leave out: as `NOT IN (NULL)`, it would keep nothing.
      const onlyNull = tested({ condition: 'not_in_list', values: [null] })
      expect(engineRequest(filtered(onlyNull)).filters).toEqual([])
    })

    it('copies the values, which are reactive', () => {
      const kept = ['Lille']
      const request = engineRequest(filtered(values(1, kept)))
      expect(request.filters[0]).toMatchObject({ values: kept })
      expect((request.filters[0] as { values: Cell[] }).values).not.toBe(kept)
    })

    it('reads by default the deepest level of the rows, with those above it', () => {
      const [filter] = engineRequest(filtered(above())).filters
      expect(filter).toEqual({
        kind: 'measure',
        measure: { aggregate: 'sum', column: 4 },
        levels: [
          { column: 0, precision: 'none' },
          { column: 1, precision: 'none' },
        ],
        test: { condition: 'greater', operand: '100', upper: '' },
      })
    })

    it('reads the level chosen, on either axis, with its precision', () => {
      const levels = (level: number, config = filtered(above({ level }))) => {
        const [filter] = engineRequest(config).filters
        return filter?.kind === 'measure' ? filter.levels.map(({ column }) => column) : []
      }
      expect(levels(0)).toEqual([0])
      expect(levels(1)).toEqual([0, 1])
      expect(levels(2)).toEqual([2])
      // A column that is no level goes back to the default level.
      expect(levels(3)).toEqual([0, 1])
      const dated = {
        ...filtered(above({ level: 2, level_precision: 'month' })),
        columns: [level(2, '', 'month')],
      }
      const [filter] = engineRequest(dated).filters
      expect(filter).toMatchObject({ levels: [{ column: 2, precision: 'month' }] })
    })

    it('tells two levels of a column by their precision', () => {
      const config = {
        ...filtered(),
        rows: [level(2, '', 'year'), level(2, '', 'month')],
        columns: [],
      }
      const levels = (level_precision: DatePrecision) => {
        const filters = [above({ level: 2, level_precision })]
        const [filter] = engineRequest({ ...config, filters }).filters
        return filter?.kind === 'measure' ? filter.levels.map(({ precision }) => precision) : []
      }
      expect(levels('year')).toEqual(['year'])
      expect(levels('month')).toEqual(['year', 'month'])
      // A precision that no level has goes back to the default level.
      expect(levels('week')).toEqual(['year', 'month'])
    })

    it('follows the level whose precision changes', () => {
      const config = {
        ...filtered(above({ level: 2 })),
        rows: [level(2)],
        columns: [],
      }
      const changed = withPrecision(config, { zone: 'rows', index: 0 }, 'month')
      expect(changed.filters).toEqual([above({ level: 2, level_precision: 'month' })])
    })

    it('reads the columns without any level of the rows, and all the rows without any level', () => {
      const onColumns = { ...filtered(above()), rows: [] }
      expect(engineRequest(onColumns).filters[0]).toMatchObject({
        levels: [{ column: 2, precision: 'none' }],
      })
      const onNothing = { ...onColumns, columns: [] }
      expect(engineRequest(onNothing).filters[0]).toMatchObject({ levels: [] })
    })
  })

  describe('activeFilterPlace', () => {
    it('finds a filter by its rank among those that have an effect', () => {
      const config = filtered(values(0, []), above({ operand: '' }), above())
      expect(activeFilterPlace(config, 0)).toEqual({ zone: 'filters', index: 2 })
    })
  })

  describe('elementExpression', () => {
    /**
     * Expression of a filter.
     *
     * @param filter Filter.
     * @param columnTypes Types of the columns.
     */
    function expression(filter: PivotFilter, columnTypes: ColumnTypeInfo[] = []): string {
      return elementExpression(filtered(filter), { zone: 'filters', index: 0 }, NAMES, columnTypes)
    }

    it('writes the values kept', () => {
      expect(expression(values(0, ['Nord', 'Sud']))).toBe("region IN ('Nord', 'Sud')")
      expect(expression(values(0, ["l'Aube"]))).toBe("region IN ('l''Aube')")
    })

    it('writes NULL apart', () => {
      expect(expression(values(0, [null]))).toBe('region IS NULL')
      expect(expression(values(0, ['Nord', null]))).toBe("region IN ('Nord') OR region IS NULL")
    })

    it('gives the name of the column without a value', () => {
      expect(expression(values(0, []))).toBe('region')
    })

    it('types the literals by the column', () => {
      const write = (family: ColumnTypeInfo['family'], value: string) =>
        expression(values(0, [value]), types(family))
      expect(write('integer', '12')).toBe('region IN (12)')
      expect(write('decimal', '1.5')).toBe('region IN (1.5)')
      expect(write('date', '2026-01-15')).toBe("region IN (DATE'2026-01-15')")
      expect(write('datetime', '2026-01-15 10:30:00')).toBe(
        "region IN (TIMESTAMP'2026-01-15 10:30:00')",
      )
      expect(write('time', '10:30:00')).toBe("region IN (TIME'10:30:00')")
    })

    it('stops a long list', () => {
      const many = Array.from({ length: 21 }, (_, index) => String(index))
      const written = expression(values(0, many), types('integer'))
      expect(written.endsWith('19, …)')).toBe(true)
    })

    it('writes a measure, its comparison and its operand', () => {
      expect(expression(above())).toBe('SUM(montant) > 100')
      expect(expression(above({ operand: '' }))).toBe('SUM(montant) > ?')
      expect(expression(above({ condition: 'not_equal' }))).toBe('SUM(montant) <> 100')
      expect(expression(above({ condition: 'between', upper: '400' }))).toBe(
        'SUM(montant) BETWEEN 100 AND 400',
      )
      expect(expression(above({ condition: 'between' }))).toBe('SUM(montant) BETWEEN 100 AND ?')
      expect(expression(above({ condition: 'is_not_null' }))).toBe('SUM(montant) IS NOT NULL')
      const dates = types('text', 'text', 'text', 'text', 'date')
      expect(expression(above({ aggregate: 'min', operand: '2026-01-15' }), dates)).toBe(
        "MIN(montant) > DATE'2026-01-15'",
      )
      expect(expression(above({ aggregate: 'count', operand: '2' }), dates)).toBe(
        'COUNT(montant) > 2',
      )
    })
  })

  describe('elementExpression of a filter on a column', () => {
    /**
     * Expression of a filter on the column `annee`.
     *
     * @param changes What differs from the filter.
     * @param family Family of the column.
     */
    function expression(
      changes: Partial<PivotColumnFilter>,
      family: ColumnTypeInfo['family'] = 'date',
    ): string {
      const dates = types('text', 'text', family)
      return elementExpression(
        filtered(tested(changes)),
        { zone: 'filters', index: 0 },
        NAMES,
        dates,
      )
    }

    it('writes NOT IN without NULL', () => {
      const condition = 'not_in_list'
      expect(expression({ condition, values: ['a', null] }, 'text')).toBe("annee NOT IN ('a')")
      expect(expression({ condition, values: [null] }, 'text')).toBe('annee')
    })

    it('writes each test', () => {
      const values = { operand: '1', upper: '5' }
      const write = (condition: PivotColumnFilter['condition']) =>
        expression({ ...values, condition }, 'integer')
      expect(write('between')).toBe('annee BETWEEN 1 AND 5')
      expect(write('not_between')).toBe('annee NOT BETWEEN 1 AND 5')
      expect(write('less')).toBe('annee < 1')
      expect(write('less_or_equal')).toBe('annee <= 1')
      expect(write('greater')).toBe('annee > 1')
      expect(write('greater_or_equal')).toBe('annee >= 1')
      expect(write('equal')).toBe('annee = 1')
      expect(write('not_equal')).toBe('annee <> 1')
      expect(write('is_null')).toBe('annee IS NULL')
      expect(write('is_not_null')).toBe('annee IS NOT NULL')
      expect(expression({}, 'integer')).toBe('annee BETWEEN ? AND ?')
    })

    it('writes a precision, with the literals of its periods', () => {
      const months = { precision: 'month', operand: '2026-01', upper: '2026-03' } as const
      expect(expression(months)).toBe("annee (Mois) BETWEEN '2026-01' AND '2026-03'")
      expect(expression({ condition: 'in_list', precision: 'quarter', values: ['2026-T1'] })).toBe(
        "annee (Trimestre) IN ('2026-T1')",
      )
      expect(expression({ condition: 'equal', precision: 'year', operand: '2026' })).toBe(
        'annee (Année) = 2026',
      )
      expect(expression({ condition: 'less', precision: 'date', operand: '2026-01-15' })).toBe(
        "annee (Date) < DATE'2026-01-15'",
      )
      expect(expression({ condition: 'is_null', precision: 'week' })).toBe(
        'annee (Semaine) IS NULL',
      )
    })
  })

  describe('elementLabel', () => {
    it('gives the name of a filter, and withName removes one equal to its expression', () => {
      const place = { zone: 'filters', index: 0 } as const
      const named = withName(filtered(above()), place, 'Gros', NAMES)
      expect(elementLabel(named, place, NAMES)).toBe('Gros')
      expect(withName(named, place, 'SUM(montant) > 100', NAMES).filters[0]?.name).toBe('')
    })
  })

  describe('placeColumn', () => {
    const to = (index: number) => ({ zone: 'filters', index }) as const

    it('gives a column of the list a filter on its values', () => {
      const placed = placeColumn(filtered(), 3, 'count', to(0), null)
      expect(placed.filters).toEqual([values(3, [])])
      // The column stays where it is on the axes.
      const again = placeColumn(filtered(), 0, 'count', to(0), null)
      expect(again.filters).toEqual([values(0, [])])
      expect(again.rows).toEqual(filtered().rows)
    })

    it('gives a column already filtered a new filter', () => {
      const config = filtered(values(3, ['a']))
      expect(placeColumn(config, 3, 'count', to(1), null).filters).toEqual([
        values(3, ['a']),
        values(3, []),
      ])
    })

    it('moves a level into the filters', () => {
      const placed = placeColumn(filtered(), 1, 'count', to(0), { zone: 'rows', index: 1 })
      expect(placed.rows).toEqual([level(0)])
      expect(placed.filters).toEqual([values(1, [])])
    })

    it('copies a measure into a filter without an operand', () => {
      const placed = placeColumn(filtered(), 4, 'count', to(0), { zone: 'measures', index: 0 })
      expect(placed.measures).toEqual([measure('sum', 4)])
      expect(placed.filters).toEqual([above({ condition: 'between', operand: '' })])
    })

    it('starts numbers and dates on BETWEEN, the other columns on the list', () => {
      const condition = (family: ColumnTypeInfo['family']) =>
        placeColumn(filtered(), 0, 'count', to(0), null, types(family)).filters[0]?.condition
      for (const family of ['integer', 'decimal', 'float', 'date', 'datetime', 'time', 'year']) {
        expect(condition(family as ColumnTypeInfo['family'])).toBe('between')
      }
      expect(condition('text')).toBe('in_list')
      expect(condition('binary')).toBe('in_list')
    })

    it('starts a measure on BETWEEN, unless it reads a text', () => {
      const config = { ...filtered(), measures: [measure('max', 0), measure('count', 0)] }
      const condition = (index: number) =>
        placeColumn(config, 0, 'count', to(0), { zone: 'measures', index }, types('text'))
          .filters[0]?.condition
      expect(condition(0)).toBe('equal')
      expect(condition(1)).toBe('between')
    })

    it('keeps the precision of a level moved into the filters, and back', () => {
      const config = { ...filtered(), rows: [level(2, '', 'month')] }
      const from = { zone: 'rows', index: 0 } as const
      const placed = placeColumn(config, 2, 'count', to(0), from, types('text', 'text', 'date'))
      expect(placed.rows).toEqual([])
      expect(placed.filters).toEqual([tested({ precision: 'month' })])
      const back = placeColumn(
        placed,
        2,
        'count',
        { zone: 'rows', index: 0 },
        { zone: 'filters', index: 0 },
      )
      expect(back.rows).toEqual([level(2, '', 'month')])
      expect(back.filters).toEqual([])
    })

    it('moves a filter among the filters', () => {
      const config = filtered(values(0, []), values(1, []), values(3, []))
      const moved = (from: number, index: number) =>
        placeColumn(config, 0, 'count', to(index), { zone: 'filters', index: from }).filters.map(
          ({ column }) => column,
        )
      expect(moved(0, 2)).toEqual([1, 0, 3])
      expect(moved(0, 3)).toEqual([1, 3, 0])
      expect(moved(2, 0)).toEqual([3, 0, 1])
    })

    it('turns a filter into a level or a measure', () => {
      const config = filtered(above({ name: 'Gros' }), values(3, []))
      const from = (index: number) => ({ zone: 'filters', index }) as const
      const level3 = placeColumn(config, 3, 'count', { zone: 'rows', index: 2 }, from(1))
      expect(level3.rows).toEqual([level(0), level(1), level(3)])
      expect(level3.filters).toEqual([above({ name: 'Gros' })])
      const measure4 = placeColumn(config, 4, 'count', { zone: 'measures', index: 1 }, from(0))
      expect(measure4.measures).toEqual([measure('sum', 4), measure('sum', 4)])
      expect(measure4.filters).toEqual([values(3, [])])
      const measure3 = placeColumn(config, 3, 'count', { zone: 'measures', index: 0 }, from(1))
      expect(measure3.measures[0]).toEqual(measure('count', 3))
    })
  })

  describe('removeElement', () => {
    it('removes a filter', () => {
      const config = filtered(values(0, []), above())
      expect(removeElement(config, { zone: 'filters', index: 0 }).filters).toEqual([above()])
    })
  })

  describe('withFilter', () => {
    it('replaces one filter', () => {
      const config = filtered(values(0, []), above())
      expect(withFilter(config, 1, above({ operand: '5' })).filters).toEqual([
        values(0, []),
        above({ operand: '5' }),
      ])
    })
  })

  describe('hasTable', () => {
    it('asks for no table with filters alone, which still make a configuration', () => {
      const config = { ...emptyConfig(), filters: [values(0, [])] }
      expect(hasTable(config)).toBe(false)
      expect(isEmptyConfig(config)).toBe(false)
      expect(hasTable(filtered())).toBe(true)
      expect(isEmptyConfig(emptyConfig())).toBe(true)
    })
  })

  describe('fittedConfig', () => {
    it('follows the columns by name, and drops the filters of a lost column', () => {
      const config = filtered(
        values(0, ['Nord']),
        values(1, ['Lille']),
        above({ level: 0 }),
        above({ aggregate: 'count_star', column: 1, level: 1 }),
        above({ aggregate: 'sum', column: 1 }),
      )
      const fitted = fittedConfig(
        { fields: NAMES, config },
        ['montant', 'region', 'annee', 'trimestre'],
        [],
      )
      expect(fitted.filters).toEqual([
        values(1, ['Nord']),
        above({ column: 0, level: 1 }),
        above({ aggregate: 'count_star', column: 0 }),
      ])
    })

    it('gives a filter its values as they are when its column holds no dates any more', () => {
      const dated = tested({ precision: 'month', operand: '2026-01', upper: '2026-03' })
      const fit = (family: ColumnTypeInfo['family']) =>
        fittedConfig(
          { fields: NAMES, config: filtered(dated) },
          NAMES,
          types('text', 'text', family),
        ).filters
      expect(fit('date')).toEqual([dated])
      expect(fit('text')).toEqual([tested()])
    })

    it('fits a configuration of filters alone', () => {
      const config = { ...emptyConfig(), filters: [values(0, ['Nord'])] }
      const fitted = fittedConfig({ fields: NAMES, config }, ['x', 'region'], [])
      expect(fitted.filters).toEqual([values(1, ['Nord'])])
    })
  })

  describe('operandRefusal', () => {
    /**
     * Refusal of an operand of `MAX` over a column of a family.
     *
     * @param family Family of the column.
     * @param operand Operand.
     */
    function refused(family: ColumnTypeInfo['family'], operand: string): boolean {
      const measured = { aggregate: 'max', column: 0 } as const
      return operandRefusal(measured, operand, types(family)) !== null
    }

    it('reads an exact number as the engine does', () => {
      for (const operand of ['100', '-1.5', '+2', '.5', '5.']) {
        expect(refused('integer', operand)).toBe(false)
        expect(refused('decimal', operand)).toBe(false)
      }
      for (const operand of ['abc', '1e3', '.', '1,5']) {
        expect(refused('decimal', operand)).toBe(true)
      }
    })

    it('reads a float, with its exponent', () => {
      expect(refused('float', '1.5e3')).toBe(false)
      expect(refused('float', 'abc')).toBe(true)
      expect(refused('float', 'Infinity')).toBe(true)
    })

    it('asks for a date of the calendar', () => {
      expect(refused('date', '2026-01-15')).toBe(false)
      expect(refused('date', '2024-02-29')).toBe(false)
      expect(refused('date', '2026-02-29')).toBe(true)
      expect(refused('date', '2026-13-01')).toBe(true)
      expect(refused('date', '0000-00-00')).toBe(true)
      expect(refused('date', '2026-1-5')).toBe(true)
    })

    it('asks for a whole instant, and for an hour', () => {
      expect(refused('datetime', '2026-01-15 10:30:00')).toBe(false)
      expect(refused('datetime', '2026-01-15 10:30:00.5')).toBe(false)
      expect(refused('datetime', '2026-01-15')).toBe(true)
      expect(refused('datetime', '2026-01-15T10:30:00')).toBe(true)
      expect(refused('datetime', '2026-01-15 24:00:00')).toBe(true)
      expect(refused('time', '10:30:00')).toBe(false)
      expect(refused('time', '23:59:59.5')).toBe(false)
      expect(refused('time', '10:30')).toBe(true)
    })

    it('asks for an hour of the day: the engine reads no other', () => {
      expect(refused('time', '-120:00:00')).toBe(true)
      expect(refused('time', '25:00:00')).toBe(true)
      expect(refused('time', '10:60:00')).toBe(true)
    })

    it('asks for a year', () => {
      expect(refused('year', '2026')).toBe(false)
      expect(refused('year', '26')).toBe(true)
    })

    it('reads the values of a filter on a column by its type', () => {
      const dates = types('text', 'text', 'date')
      expect(operandRefusal(tested(), '2026-01-15', dates)).toBeNull()
      expect(operandRefusal(tested(), 'abc', dates)).toBe('Une date est attendue, comme 2026-01-15')
      expect(operandRefusal(tested(), 'abc', [])).toBeNull()
    })

    it('reads a value of a precision as the labels of its periods', () => {
      const dates = types('text', 'text', 'datetime')
      const refusal = (precision: DatePrecision, operand: string) =>
        operandRefusal(tested({ precision }), operand, dates)
      expect(refusal('year', '2026')).toBeNull()
      expect(refusal('year', '0000')).toBe('Une année est attendue, comme 2026')
      expect(refusal('quarter', '2026-T4')).toBeNull()
      expect(refusal('quarter', '2026-T5')).toBe('Un trimestre est attendu, comme 2026-T1')
      expect(refusal('month', '2026-12')).toBeNull()
      expect(refusal('month', '2026-13')).toBe('Un mois est attendu, comme 2026-01')
      expect(refusal('week', '2026-S53')).toBeNull()
      expect(refusal('week', '2026-S54')).toBe('Une semaine est attendue, comme 2026-S03')
      expect(refusal('date', '2026-01-15')).toBeNull()
      expect(refusal('date', '2026-02-30')).toBe('Une date est attendue, comme 2026-01-15')
      expect(refusal('none', '2026-01-15')).toBe(
        'Un instant est attendu, comme 2026-01-15 10:30:00',
      )
    })

    it('takes any text', () => {
      expect(refused('text', 'abc')).toBe(false)
    })

    it('reads a count as a number, whatever its column', () => {
      const count = { aggregate: 'count', column: 0 } as const
      expect(operandRefusal(count, 'abc', types('text'))).not.toBeNull()
      expect(operandRefusal(count, '2', types('text'))).toBeNull()
    })
  })

  describe('operandExample', () => {
    it('shows the form the filter awaits', () => {
      const dates = types('text', 'text', 'date')
      expect(operandExample(tested(), dates)).toBe('2026-01-15')
      expect(operandExample(tested({ precision: 'quarter' }), dates)).toBe('2026-T1')
      expect(operandExample(above(), types('text', 'text', 'text', 'text', 'integer'))).toBe('100')
      expect(operandExample(tested({ column: 0 }), dates)).toBe('')
    })
  })

  describe('isRefused', () => {
    const dates = types('text', 'text', 'date')

    it('reads only the values its condition asks for', () => {
      expect(isRefused(tested({ operand: 'abc' }), dates)).toBe(true)
      expect(isRefused(tested({ upper: 'abc' }), dates)).toBe(true)
      expect(isRefused(tested({ condition: 'less', upper: 'abc' }), dates)).toBe(false)
      expect(isRefused(tested({ condition: 'is_null', operand: 'abc' }), dates)).toBe(false)
      expect(isRefused(tested({ condition: 'in_list', operand: 'abc' }), dates)).toBe(false)
      expect(isRefused(tested(), dates)).toBe(false)
    })
  })
})

describe('the layout', () => {
  /**
   * Three levels of rows and two of columns.
   */
  const LEVELS: PivotConfig = {
    rows: [level(0), level(1), level(2)],
    columns: [level(3), level(4)],
    measures: [measure('count_star', 0)],
    filters: [],
  }

  it('asks for the compact form and every total, as a table just computed', () => {
    expect(engineLayout(LEVELS)).toEqual({
      form: 'compact',
      rows: { grandTotal: true, subtotals: [] },
      columns: { grandTotal: true, subtotals: [] },
    })
    expect(hasGrandTotal(LEVELS, 'rows')).toBe(true)
  })

  it('asks for what is set, each axis apart', () => {
    let config = withForm(LEVELS, 'tabular')
    config = withGrandTotal(config, 'columns', false)
    config = withSubtotal(config, 'rows', 1, false)
    expect(engineLayout(config)).toEqual({
      form: 'tabular',
      rows: { grandTotal: true, subtotals: [true, false, true] },
      columns: { grandTotal: false, subtotals: [] },
    })
    expect(hasGrandTotal(config, 'columns')).toBe(false)
    expect(hasGrandTotal(config, 'rows')).toBe(true)
  })

  it('gives every subtotal back, or none', () => {
    const none = withAllSubtotals(LEVELS, 'rows', false)
    expect(engineLayout(none).rows.subtotals).toEqual([false, false, false])
    expect(engineLayout(none).columns.subtotals).toEqual([])
    const all = withAllSubtotals(none, 'rows', true)
    expect(engineLayout(all).rows.subtotals).toEqual([])
    expect(sameTableLayout(engineLayout(all), engineLayout(LEVELS))).toBe(true)
    expect(sameTableLayout(engineLayout(none), engineLayout(LEVELS))).toBe(false)
  })

  it('computes nothing: the request stays the same', () => {
    let config = withForm(LEVELS, 'outline')
    config = withRepeatedLabels(config, true)
    config = withGrandTotal(config, 'rows', false)
    config = withAllSubtotals(config, 'columns', false)
    expect(sameRequest(engineRequest(config), engineRequest(LEVELS))).toBe(true)
    expect(sameOrder(engineOrder(config), engineOrder(LEVELS))).toBe(true)
  })

  it('reads the repetition of the labels apart from the layout of the engine', () => {
    const repeated = withRepeatedLabels(LEVELS, true)
    expect(repeated.repeat_labels).toBe(true)
    expect(withRepeatedLabels(repeated, false).repeat_labels).toBe(false)
    expect(sameTableLayout(engineLayout(repeated), engineLayout(LEVELS))).toBe(true)
  })

  it('lets the subtotal of a level follow it to its new place and its new axis', () => {
    const config = withSubtotal(LEVELS, 'rows', 0, false)
    const moved = placeColumn(
      config,
      0,
      'count',
      { zone: 'columns', index: 0 },
      { zone: 'rows', index: 0 },
    )
    expect(engineLayout(moved).rows.subtotals).toEqual([])
    expect(engineLayout(moved).columns.subtotals).toEqual([false, true, true])
  })

  it('keeps the layout when the columns of the result change', () => {
    const config = withSubtotal(
      withGrandTotal(withRepeatedLabels(withForm(LEVELS, 'tabular'), true), 'rows', false),
      'columns',
      0,
      false,
    )
    const moved = ['montant', 'trimestre', 'annee', 'ville', 'region']
    const fitted = fittedConfig({ fields: NAMES, config }, moved, [])
    expect(fitted.form).toBe('tabular')
    expect(fitted.repeat_labels).toBe(true)
    expect(hasGrandTotal(fitted, 'rows')).toBe(false)
    expect(fitted.rows.map(({ column }) => column)).toEqual([4, 3, 2])
    expect(fitted.columns[0]?.without_subtotal).toBe(true)
  })
})

describe('the widths', () => {
  /**
   * Lines by region and ville, columns by annee, two measures: the column 2025 of the second
   * measure and the grand total of the first one have their width.
   */
  const WIDENED: PivotConfig = {
    rows: [level(0), level(1)],
    columns: [level(2)],
    measures: [measure('sum', 4), measure('count', 4)],
    filters: [],
    column_widths: [
      { path: ['2025'], measure: 1, width: 50 },
      { path: [], measure: 0, width: 20 },
    ],
  }

  it('gives a column of values its width, named by its values, and gives it back', () => {
    const wider = withColumnWidth(WIDENED, { path: ['2025'], measure: 1 }, 60)
    expect(wider.column_widths).toEqual([
      { path: [], measure: 0, width: 20 },
      { path: ['2025'], measure: 1, width: 60 },
    ])
    expect(withColumnWidth(WIDENED, { path: [null], measure: 0 }, 30).column_widths).toHaveLength(3)
    const narrower = withColumnWidth(WIDENED, { path: ['2025'], measure: 1 }, null)
    expect(narrower.column_widths).toEqual([{ path: [], measure: 0, width: 20 }])
    expect(withColumnWidth(narrower, { path: [], measure: 0 }, null).column_widths).toBeUndefined()
  })

  it('gives the labels their width, in a single column or per level', () => {
    const single = withLabelWidth(WIDENED, null, 70)
    expect(single.labels_width).toBe(70)
    expect(withLabelWidth(single, null, null).labels_width).toBeUndefined()
    const second = withLabelWidth(WIDENED, 1, 45)
    expect(second.rows.map(({ width }) => width)).toEqual([undefined, 45])
    expect(withLabelWidth(second, 1, null).rows[1]?.width).toBeUndefined()
  })

  it('names the columns given a width for the engine, in copies the Worker can receive', () => {
    const targets = engineTargets(reactive(structuredClone(WIDENED)))
    expect(targets).toEqual([
      { path: ['2025'], measure: 1 },
      { path: [], measure: 0 },
    ])
    expect(() => structuredClone(targets)).not.toThrow()
    expect(engineTargets({ ...WIDENED, column_widths: undefined })).toEqual([])
  })

  it('computes nothing: the request, the order and the layout stay the same', () => {
    const config = withLabelWidth(
      withColumnWidth(WIDENED, { path: ['2026'], measure: 0 }, 40),
      0,
      30,
    )
    expect(sameRequest(engineRequest(config), engineRequest(WIDENED))).toBe(true)
    expect(sameOrder(engineOrder(config), engineOrder(WIDENED))).toBe(true)
    expect(sameTableLayout(engineLayout(config), engineLayout(WIDENED))).toBe(true)
  })

  it('follows the measure of a column, and drops its width without it', () => {
    const moved = placeColumn(
      WIDENED,
      4,
      'count',
      { zone: 'measures', index: 0 },
      { zone: 'measures', index: 1 },
    )
    expect(moved.measures).toEqual([measure('count', 4), measure('sum', 4)])
    expect(moved.column_widths).toEqual([
      { path: ['2025'], measure: 0, width: 50 },
      { path: [], measure: 1, width: 20 },
    ])
    const removed = removeElement(WIDENED, { zone: 'measures', index: 1 })
    expect(removed.column_widths).toEqual([{ path: [], measure: 0, width: 20 }])
  })

  it('drops the widths with a change of the levels of the columns, not of the rows', () => {
    const added = placeColumn(WIDENED, 3, 'sum', { zone: 'columns', index: 1 }, null)
    expect(added.column_widths).toBeUndefined()
    const month = withPrecision(WIDENED, { zone: 'columns', index: 0 }, 'month')
    expect(month.column_widths).toBeUndefined()
    const rows = removeElement(WIDENED, { zone: 'rows', index: 1 })
    expect(rows.column_widths).toEqual(WIDENED.column_widths)
  })

  it('lets the width of a level follow it to its new place', () => {
    const config = withLabelWidth(WIDENED, 0, 33)
    const moved = placeColumn(
      config,
      0,
      'sum',
      { zone: 'columns', index: 0 },
      { zone: 'rows', index: 0 },
    )
    expect(moved.columns[0]?.width).toBe(33)
    expect(moved.rows.map(({ width }) => width)).toEqual([undefined])
  })

  it('keeps the widths when the columns of the result follow their names', () => {
    const config = withLabelWidth(WIDENED, null, 70)
    const moved = ['montant', 'region', 'ville', 'annee', 'trimestre']
    const fitted = fittedConfig({ fields: NAMES, config }, moved, [])
    expect(fitted.column_widths).toEqual(WIDENED.column_widths)
    expect(fitted.labels_width).toBe(70)
    const lost = ['region', 'annee', 'trimestre', 'montant']
    expect(fittedConfig({ fields: NAMES, config }, lost, []).column_widths).toBeUndefined()
  })
})
