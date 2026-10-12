import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiError, request } from '../http'
import { streamCatalog } from '../explorer'
import type { CatalogEvent } from '../generated/CatalogEvent'

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

describe('streamCatalog', () => {
  it('transmet chaque ligne dès qu’elle est complète, même coupée entre deux paquets', async () => {
    const chunks = [
      '{"type":"schemas","schemas":["a"]}\n{"type":"tab',
      'les","schema":"a","tables":[]}\n',
      '{"type":"done"}\n',
    ]
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        for (const chunk of chunks) {
          controller.enqueue(new TextEncoder().encode(chunk))
        }
        controller.close()
      },
    })
    const fake = vi.fn<(path: string) => Promise<Response>>(async () => new Response(body))
    vi.stubGlobal('fetch', fake)

    const events: CatalogEvent[] = []
    await streamCatalog(3, (event) => events.push(event))

    expect(fake.mock.calls[0]?.[0]).toBe('/api/connections/3/catalog')
    expect(events.map((event) => event.type)).toEqual(['schemas', 'tables', 'done'])
  })

  it('lève une ApiError si le serveur cible est injoignable', async () => {
    mockFetch(504, '{"error":"192.168.0.4:3307 ne répond pas"}')
    await expect(streamCatalog(3, () => {})).rejects.toThrow('192.168.0.4:3307 ne répond pas')
  })
})
