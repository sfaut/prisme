import { describe, expect, it } from 'vitest'

import { writeByChunks } from '../chunks'

describe('writeByChunks', () => {
  it('writes the rows in order, by chunks of 2,000', async () => {
    const parts = await writeByChunks(4500, (row) => `${row}\n`)
    expect(parts).toHaveLength(3)
    expect(parts.join('').split('\n').slice(0, -1).map(Number)).toEqual(
      Array.from({ length: 4500 }, (_, row) => row),
    )
  })

  it('writes nothing without rows', async () => {
    expect(await writeByChunks(0, () => 'x')).toEqual([])
  })
})
