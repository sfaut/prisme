import { describe, expect, it } from 'vitest'

import { detectContent, isWebAddress } from '../contentKind'

/**
 * Rows of a single column.
 *
 * @param values Values of the column.
 */
function column(...values: (string | null)[]): (string | null)[][] {
  return values.map((value) => [value])
}

describe('detectContent', () => {
  it('recognizes a column of JSON documents, NULL and empty values ignored', () => {
    expect(detectContent(column('{"a": 1}', null, '', ' [1, 2] '), 0)).toBe('json')
  })

  it('recognizes a column of XML documents', () => {
    expect(detectContent(column('<a><b/></a>', '<?xml version="1.0"?><r x="1"/>'), 0)).toBe('xml')
  })

  it('recognizes a column of web addresses, http or https only', () => {
    expect(detectContent(column('https://example.com/a?b=1', 'http://x.fr'), 0)).toBe('url')
    expect(detectContent(column('https://example.com', 'javascript:alert(1)'), 0)).toBeNull()
    expect(isWebAddress('https://ok.fr/chemin')).toBe(true)
    expect(isWebAddress('https://pas ok.fr')).toBe(false)
    expect(isWebAddress(null)).toBe(false)
  })

  it('leaves as text a mixed column, JSON scalars or malformed XML', () => {
    expect(detectContent(column('{"a": 1}', 'texte'), 0)).toBeNull()
    expect(detectContent(column('42', '"string"'), 0)).toBeNull()
    expect(detectContent(column('<a><b></a>'), 0)).toBeNull()
    expect(detectContent(column('{pas du json}'), 0)).toBeNull()
    expect(detectContent(column(null, ''), 0)).toBeNull()
  })
})
