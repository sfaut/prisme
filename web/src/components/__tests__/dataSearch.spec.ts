import { describe, expect, it } from 'vitest'

import { dataDocument } from '../dataFormat'
import { type SearchResult, searchData } from '../dataSearch'

/**
 * Document shown for some values, as the explorer builds it.
 *
 * @param kind Format of the values.
 * @param values Values of the cells.
 */
function shown(kind: 'json' | 'xml', ...values: (string | null)[]): string {
  const document = dataDocument(
    kind,
    values.map((value, index) => ({ row: index + 1, value })),
  )
  if (!('text' in document)) {
    throw new Error(document.message)
  }
  return document.text
}

/**
 * Texts of the highlighted results.
 *
 * @param text Document.
 * @param result Result of the search.
 */
function matched(text: string, result: SearchResult): string[] {
  if (result.state !== 'nodes') {
    throw new Error(`no nodes: ${result.state}`)
  }
  return result.matches.map((range) => text.slice(range.from, range.to))
}

describe('searchData, JSON', () => {
  it('finds nothing without an expression', () => {
    expect(searchData('json', '{}', '  ')).toEqual({ state: 'empty' })
  })

  it('highlights the values found, members of objects and elements of arrays', () => {
    const text = shown('json', '{"users":[{"name":"Alice","age":30},{"name":"Bob","age":25}]}')
    expect(matched(text, searchData('json', text, '$.users[*].name'))).toEqual(['"Alice"', '"Bob"'])
    expect(matched(text, searchData('json', text, '$.users[?@.age > 26]'))).toEqual([
      '{\n            "name": "Alice",\n            "age": 30\n        }',
    ])
  })

  it('shows the results as an array, numbers as written', () => {
    const text = shown('json', '{"a":1.50,"b":12345678901234567890,"c":[1,2]}')
    const result = searchData('json', text, '$.*')
    expect(result).toMatchObject({ state: 'nodes', count: 3 })
    expect(result.state === 'nodes' && result.text).toBe(
      '[\n    1.50,\n    12345678901234567890,\n    [\n        1,\n        2\n    ]\n]',
    )
  })

  it('searches the values gathered as one array', () => {
    const text = shown('json', '{"a":1}', null, '{"a":2}')
    expect(matched(text, searchData('json', text, '$[*].a'))).toEqual(['1', '2'])
    expect(matched(text, searchData('json', text, '$[1]'))).toEqual(['null'])
  })

  it('counts a node found twice, highlights it once', () => {
    const text = shown('json', '[10,20]')
    const result = searchData('json', text, '$[0,0]')
    expect(result).toMatchObject({ state: 'nodes', count: 2 })
    expect(matched(text, result)).toEqual(['10'])
  })

  it('tells a key "0" from an index 0, and finds keys to escape', () => {
    const text = shown('json', '{"0":"key","a-b":[true]}')
    expect(matched(text, searchData('json', text, "$['0']"))).toEqual(['"key"'])
    expect(matched(text, searchData('json', text, "$['a-b'][0]"))).toEqual(['true'])
  })

  it('finds nothing as an empty array', () => {
    const text = shown('json', '{"a":1}')
    expect(searchData('json', text, '$.b')).toEqual({
      state: 'nodes',
      count: 0,
      matches: [],
      text: '[]',
    })
  })

  it('reports an invalid expression', () => {
    const result = searchData('json', '{}', '$[?')
    expect(result.state).toBe('error')
    expect(result.state === 'error' && result.message).toMatch(/^JSONPath invalide : /)
  })

  it('refuses the extensions outside RFC 9535', () => {
    expect(searchData('json', '{"a":1}', '$.~').state).toBe('error')
  })
})

describe('searchData, XML', () => {
  it('highlights the elements and the attributes found', () => {
    const text = shown(
      'xml',
      '<clients><client id="1"><nom>A</nom></client><client id="2"/></clients>',
    )
    expect(matched(text, searchData('xml', text, '//nom'))).toEqual(['<nom>A</nom>'])
    expect(matched(text, searchData('xml', text, '//client/@id'))).toEqual(['id="1"', 'id="2"'])
  })

  it('leaves out the indentation of the explorer, keeps the blanks of the values', () => {
    const text = shown('xml', '<a><b>x</b><c xml:space="preserve"> <d/> </c></a>')
    const result = searchData('xml', text, '//text()')
    expect(result).toMatchObject({ state: 'nodes', count: 3 })
    expect(matched(text, result)).toEqual(['x', ' ', ' '])
    expect(searchData('xml', text, 'count(/a/node())')).toEqual({ state: 'value', text: '2' })
  })

  it('shows the results in <data>, attributes and texts as text', () => {
    const text = shown('xml', '<a x="1 &amp; 2"><b><c/></b>t&lt;u</a>')
    expect(searchData('xml', text, '//b | //@x | /a/text()')).toMatchObject({
      text: '<data>\n    1 &amp; 2\n    <b>\n        <c/>\n    </b>\n    t&lt;u\n</data>',
    })
    expect(searchData('xml', text, '//z')).toMatchObject({ count: 0, text: '<data/>' })
  })

  it('searches the values gathered in <data>', () => {
    const text = shown('xml', '<a>1</a>', null, '<a>2</a>')
    expect(matched(text, searchData('xml', text, '/data/a'))).toEqual(['<a>1</a>', '<a>2</a>'])
  })

  it('gives the value of a number, string or boolean expression', () => {
    const text = shown('xml', '<a><b>x</b><b>y</b></a>')
    expect(searchData('xml', text, 'count(//b)')).toEqual({ state: 'value', text: '2' })
    expect(searchData('xml', text, 'string(//b[2])')).toEqual({ state: 'value', text: 'y' })
    expect(searchData('xml', text, 'boolean(//c)')).toEqual({ state: 'value', text: 'false' })
  })

  it('resolves the prefixes declared in the document', () => {
    const text = shown('xml', '<r xmlns:p="urn:p"><p:a/><a/></r>')
    expect(matched(text, searchData('xml', text, '//p:a'))).toEqual(['<p:a/>'])
  })

  it('finds an attribute whose name appears in the value of another', () => {
    const text = shown('xml', `<a b="c d='x'" d="y"/>`)
    expect(matched(text, searchData('xml', text, '//@d'))).toEqual(['d="y"'])
  })

  it('reports an invalid expression', () => {
    const result = searchData('xml', '<a/>', '//[')
    expect(result.state).toBe('error')
    expect(result.state === 'error' && result.message).toMatch(/^XPath invalide : /)
  })
})
