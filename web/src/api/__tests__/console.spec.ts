import { beforeEach, describe, expect, it, vi } from 'vitest'

import { clearDebug, clearHistory, listDebug, listHistory } from '../console'

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
    await listDebug({ level: ['error', 'warn'], source: ['interface'] }, null, 12, 500)
    expect(request).toHaveBeenCalledWith(
      'GET',
      '/api/console/debug?level=error&level=warn&source=interface&before=12&limit=500',
    )
  })

  it('leaves out the filters that keep everything', async () => {
    await listHistory({ action: [] }, null)
    expect(request).toHaveBeenCalledWith('GET', '/api/console/history')
  })

  it('reads and clears the elements of an editor', async () => {
    await listHistory({ action: ['run'] }, 3)
    expect(request).toHaveBeenCalledWith('GET', '/api/console/history?action=run&editor=3')
    await clearHistory(3)
    expect(request).toHaveBeenCalledWith('DELETE', '/api/console/history?editor=3')
    await clearDebug(null)
    expect(request).toHaveBeenCalledWith('DELETE', '/api/console/debug')
  })
})
