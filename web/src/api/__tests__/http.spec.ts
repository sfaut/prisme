import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiError, UNREACHABLE, request } from '../http'
import { beautify } from '../beautify'
import { streamCatalog } from '../explorer'
import type { CatalogEvent } from '../generated/CatalogEvent'

/**
 * Replaces `fetch` with a fixed response and returns the fake `fetch`, to inspect its calls.
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
  it('returns the JSON body', async () => {
    mockFetch(200, '[{"id":1}]')
    await expect(request('GET', '/api/connections')).resolves.toEqual([{ id: 1 }])
  })

  it('sends the body as JSON', async () => {
    const fake = mockFetch(201, '{"id":3}')
    await request('POST', '/api/connections', { name: 'prod' })
    expect(fake).toHaveBeenCalledWith('/api/connections', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{"name":"prod"}',
      keepalive: false,
    })
  })

  it('lets a request outlive the page when asked to', async () => {
    const fake = mockFetch(200, '')
    await request('PUT', '/api/editors/1', {}, { keepalive: true })
    expect(fake).toHaveBeenCalledWith(
      '/api/editors/1',
      expect.objectContaining({ keepalive: true }),
    )
  })

  it('throws an ApiError with the message written from the detail of the server', async () => {
    mockFetch(
      409,
      JSON.stringify({
        error: 'a connection named "prod" already exists',
        detail: { code: 'duplicate_connection_name', name: 'prod' },
      }),
    )
    const error = await request('POST', '/api/connections', {}).catch((error: unknown) => error)
    expect(error).toBeInstanceOf(ApiError)
    expect((error as ApiError).status).toBe(409)
    expect((error as ApiError).message).toBe('Une connexion nommée « prod » existe déjà.')
    expect((error as ApiError).detail?.code).toBe('duplicate_connection_name')
  })

  it('quotes an answer that is not the JSON of Prisme, with its status', async () => {
    mockFetch(422, 'Failed to deserialize the JSON body')
    await expect(request('GET', '/api/health')).rejects.toThrow(
      'Le serveur Prisme a refusé la requête (statut 422) :\n\nFailed to deserialize the JSON body',
    )
  })

  it('tells that the server is out of reach when the network fails', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => {
        throw new TypeError('Failed to fetch')
      }),
    )
    await expect(request('GET', '/api/health')).rejects.toThrow(UNREACHABLE)
  })
})

describe('streamCatalog', () => {
  it('passes on each line as soon as it is complete, even split between two chunks', async () => {
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

  it('throws an ApiError if the target server is unreachable', async () => {
    mockFetch(
      504,
      JSON.stringify({
        error: '192.168.0.4:3307 does not answer',
        detail: {
          code: 'connect',
          failure: { reason: 'timeout', address: '192.168.0.4:3307', seconds: 10 },
        },
      }),
    )
    await expect(streamCatalog(3, () => {})).rejects.toThrow(
      '192.168.0.4:3307 ne répond pas (délai de 10 s dépassé)',
    )
  })
})

describe('beautify', () => {
  it('returns the text beautified by the server', async () => {
    const fake = mockFetch(200, '{"sql":"SELECT a FROM t"}')
    await expect(beautify('select a from t')).resolves.toBe('SELECT a FROM t')
    expect(fake.mock.calls[0]?.[0]).toBe('/api/beautify')
  })

  it('sends the indentation of the editor', async () => {
    const fake = mockFetch(200, '{"sql":"SELECT ALL\\n  a,\\n  b\\nFROM t"}')
    await beautify('select a, b from t', { indent_size: 2 })
    expect(fake.mock.calls[0]?.[1]?.body).toBe('{"sql":"select a, b from t","indent_size":2}')
  })
})
