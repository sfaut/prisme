import { describe, expect, it } from 'vitest'

import {
  type DatePrecision,
  type NamedMeasure,
  type PivotConfig,
  type PivotLevel,
  addsNumbers,
  columnAt,
  defaultAggregate,
  elementLabel,
  emptyConfig,
  engineRequest,
  fittedConfig,
  isEmptyConfig,
  measureExpression,
  placeColumn,
  precisionChoices,
  removeElement,
  sameRequest,
  withAggregate,
  withName,
  withPrecision,
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
    }
    const request = engineRequest(config)
    expect(request).toEqual({
      rows: [{ column: 0, precision: 'none' }],
      columns: [{ column: 2, precision: 'none' }],
      measures: [{ aggregate: 'sum', column: 4 }],
    })
    expect(
      sameRequest(request, engineRequest(withName(config, { zone: 'rows', index: 0 }, 'R', NAMES))),
    ).toBe(true)
    expect(sameRequest(request, engineRequest(withAggregate(config, 0, 'count')))).toBe(false)
  })

  it('asks for another table when a precision changes', () => {
    const config: PivotConfig = { rows: [level(0)], columns: [], measures: [] }
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
    })
    expect(isEmptyConfig(config)).toBe(false)
    expect(isEmptyConfig(emptyConfig())).toBe(true)
  })

  it('moves a level already placed, with its name, instead of placing it twice', () => {
    const config: PivotConfig = {
      rows: [level(0, 'Région'), level(1)],
      columns: [level(2)],
      measures: [],
    }
    expect(placeColumn(config, 0, 'count', { zone: 'columns', index: 0 }, null)).toEqual({
      rows: [level(1)],
      columns: [level(0, 'Région'), level(2)],
      measures: [],
    })
    expect(placeColumn(config, 0, 'count', { zone: 'rows', index: 2 }, null).rows).toEqual([
      level(1),
      level(0, 'Région'),
    ])
  })

  it('moves an element within its zone, the target counted before the move', () => {
    const config: PivotConfig = { rows: [level(0), level(1), level(2)], columns: [], measures: [] }
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
    })
  })
})

describe('removeElement', () => {
  it('removes one element of one zone', () => {
    const config: PivotConfig = {
      rows: [level(0), level(1)],
      columns: [level(2)],
      measures: [measure('sum', 4)],
    }
    expect(removeElement(config, { zone: 'rows', index: 0 })).toEqual({
      rows: [level(1)],
      columns: [level(2)],
      measures: config.measures,
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
    })
  })

  it('removes the elements of a lost column, except COUNT(*), which reads none', () => {
    expect(fittedConfig({ fields: NAMES, config }, ['ville', 'trimestre'], [])).toEqual({
      rows: [],
      columns: [level(0)],
      measures: [measure('count_star', 0)],
    })
  })

  it('takes the n-th column of a repeated name for the n-th', () => {
    const fields = ['id', 'id', 'x']
    const repeated: PivotConfig = {
      rows: [level(1)],
      columns: [],
      measures: [measure('count', 0)],
    }
    expect(fittedConfig({ fields, config: repeated }, ['x', 'id', 'id'], [])).toEqual({
      rows: [level(2)],
      columns: [],
      measures: [measure('count', 1)],
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
    }
    expect(withAggregate(config, 1, 'count').measures).toEqual([
      measure('sum', 4),
      measure('count', 2, 'Années'),
    ])
  })
})
