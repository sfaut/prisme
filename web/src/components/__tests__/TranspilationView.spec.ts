import { describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'

import type { TranspilationReport } from '@/api/generated/TranspilationReport'

import TranspilationView from '../TranspilationView.vue'

/**
 * Query of the example of the dialects, written in MySQL.
 */
const SQL = 'WITH ds (c) AS (VALUES ROW (123), ROW (234), ROW (345)) SELECT * FROM ds'

/**
 * Report of its transpilation towards MariaDB.
 */
const REPORT: TranspilationReport = {
  input: 'mysql 8.0.19',
  output: 'mariadb 11.3.2',
  sql: 'WITH ds (c) AS (VALUES (123), (234), (345)) SELECT * FROM ds',
  steps: [
    {
      rule: 'values.form',
      kind: 'rewrite',
      message: 'MariaDB writes the rows of VALUES without ROW',
      span: { start: 16, end: 54 },
    },
  ],
  errors: [],
}

describe('TranspilationView', () => {
  it('sums up the transpilation, folded, then shows each step with its passage', () => {
    const wrapper = mount(TranspilationView, { props: { report: REPORT, sql: SQL } })
    expect(wrapper.find('summary').text()).toContain('MySQL 8.0.19 → MariaDB 11.3.2 · 1 étape')
    expect(wrapper.find('details').attributes('open')).toBeUndefined()
    expect(wrapper.find('pre').text()).toBe(REPORT.sql)
    const step = wrapper.find('li').text()
    expect(step).toContain('Réécriture')
    expect(step).toContain('values.form')
    expect(step).toContain('« VALUES ROW (123), ROW (234), ROW (345) »')
  })

  it('unfolds a query that cannot be transpiled', () => {
    const report: TranspilationReport = {
      ...REPORT,
      sql: '',
      steps: [],
      errors: [{ rule: null, message: 'unknown table `t`', span: { start: 14, end: 15 } }],
    }
    const wrapper = mount(TranspilationView, { props: { report, sql: 'SELECT * FROM t' } })
    expect(wrapper.find('details').attributes('open')).toBeDefined()
    expect(wrapper.find('summary').text()).toContain('sans différence · 1 erreur')
    expect(wrapper.find('pre').exists()).toBe(false)
    expect(wrapper.find('li').text()).toContain('unknown table `t`')
  })
})
