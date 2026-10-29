import { describe, expect, it } from 'vitest'

import { downloadName } from '../download'

describe('download name', () => {
  it('adds the local date and time to the label', () => {
    expect(downloadName('ventes', 'csv', new Date(2026, 9, 29, 8, 5, 3))).toBe(
      'ventes-20261029-080503.csv',
    )
  })

  it('replaces the characters a file name cannot hold', () => {
    expect(downloadName(' a/b:c*?"<>|d\\e ', 'csv', new Date(2026, 0, 1))).toBe(
      'a_b_c______d_e-20260101-000000.csv',
    )
  })
})
