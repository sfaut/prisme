import { beforeEach, describe, expect, it, vi } from 'vitest'

import { listDebug, listHistory } from '../console'

const request = vi.fn<(method: string, path: string) => Promise<unknown>>()

vi.mock('../http', () => ({
  request: (method: string, path: string) => request(method, path),
}))

beforeEach(() => {
  request.mockReset()
  request.mockResolvedValue([])
})

describe('console API', () => {
  it('repeats the key of a filter for each value kept', async () => {
    await listDebug({ level: ['error', 'warn'], source: ['interface'] }, 12, 500)
    expect(request).toHaveBeenCalledWith(
      'GET',
      '/api/console/debug?level=error&level=warn&source=interface&before=12&limit=500',
    )
  })

  it('leaves out the filters that keep everything', async () => {
    await listHistory({ action: [] })
    expect(request).toHaveBeenCalledWith('GET', '/api/console/history')
  })
})
