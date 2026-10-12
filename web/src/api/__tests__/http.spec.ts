import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiError, request } from '../http'
import { loadCatalog } from '../explorer'

/**
 * Remplace `fetch` par une réponse fixe et renvoie le faux `fetch`, pour inspecter ses appels.
 */
function mockFetch(status: number, body: string) {
  const fake = vi.fn<(path: string, init: RequestInit) => Promise<Response>>(
    async () => new Response(body, { status, statusText: 'Bad Gateway' }),
  )
  vi.stubGlobal('fetch', fake)
  return fake
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('request', () => {
  it('renvoie le corps JSON', async () => {
    mockFetch(200, '[{"id":1}]')
    await expect(request('GET', '/api/connections')).resolves.toEqual([{ id: 1 }])
  })

  it('envoie le corps en JSON', async () => {
    const fake = mockFetch(201, '{"id":3}')
    await request('POST', '/api/connections', { name: 'prod' })
    expect(fake).toHaveBeenCalledWith('/api/connections', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{"name":"prod"}',
    })
  })

  it('lève une ApiError avec le message du serveur', async () => {
    mockFetch(409, '{"error":"une connexion nommée « prod » existe déjà"}')
    const error = await request('POST', '/api/connections', {}).catch((error: unknown) => error)
    expect(error).toBeInstanceOf(ApiError)
    expect((error as ApiError).status).toBe(409)
    expect((error as ApiError).message).toBe('une connexion nommée « prod » existe déjà')
  })

  it("se rabat sur le statut si le corps n'est pas du JSON", async () => {
    mockFetch(502, 'proxy indisponible')
    await expect(request('GET', '/api/health')).rejects.toThrow('502 Bad Gateway')
  })
})

describe('explorer', () => {
  it('lit le catalogue de la connexion', async () => {
    const fake = mockFetch(200, '[]')
    await expect(loadCatalog(3)).resolves.toEqual([])
    expect(fake.mock.calls[0]?.[0]).toBe('/api/connections/3/catalog')
  })
})
