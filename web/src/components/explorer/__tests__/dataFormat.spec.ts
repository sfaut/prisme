import { describe, expect, it } from 'vitest'

import { dataDocument, reindentJson } from '../dataFormat'

describe('JSON of the data explorer', () => {
  it('is indented with four spaces, numbers and strings as written', () => {
    expect(reindentJson('{"a":1.50,"b":[12345678901234567890,true,null],"c":"x, y: {z}"}')).toBe(
      [
        '{',
        '    "a": 1.50,',
        '    "b": [',
        '        12345678901234567890,',
        '        true,',
        '        null',
        '    ],',
        '    "c": "x, y: {z}"',
        '}',
      ].join('\n'),
    )
  })

  it('keeps escaped quotes, duplicate keys and empty containers', () => {
    expect(reindentJson('{ "a\\"b" : {}, "a\\"b" : [ ] }')).toBe(
      '{\n    "a\\"b": {},\n    "a\\"b": []\n}',
    )
  })

  it('shows one value alone, several in an array where NULL becomes null', () => {
    expect(dataDocument('json', [{ row: 1, value: '[1]' }])).toEqual({ text: '[\n    1\n]' })
    expect(
      dataDocument('json', [
        { row: 1, value: '{"a":1}' },
        { row: 3, value: null },
      ]),
    ).toEqual({ text: '[\n    {\n        "a": 1\n    },\n    null\n]' })
  })

  it('names the row of an invalid value', () => {
    expect(
      dataDocument('json', [
        { row: 1, value: '{}' },
        { row: 4, value: '{"a":' },
      ]),
    ).toEqual({ message: 'La valeur de la ligne 4 n’est pas du JSON valide.' })
  })
})

describe('XML of the data explorer', () => {
  it('indents the elements, and leaves text and mixed content as written', () => {
    const value =
      '<?xml version="1.0"?><order id="a>b"><!-- note --><line qty="2"><name>Pen  &amp; ink</name></line><p>Hello <b>you</b> !</p><empty/></order>'
    expect(dataDocument('xml', [{ row: 1, value }])).toEqual({
      text: [
        '<?xml version="1.0"?>',
        '<order id="a>b">',
        '    <!-- note -->',
        '    <line qty="2">',
        '        <name>Pen  &amp; ink</name>',
        '    </line>',
        '    <p>Hello <b>you</b> !</p>',
        '    <empty/>',
        '</order>',
      ].join('\n'),
    })
  })

  it('replaces the blanks between elements, except under xml:space="preserve"', () => {
    expect(dataDocument('xml', [{ row: 1, value: '<a>\n  <b/>\n\t<c><d/></c></a>' }])).toEqual({
      text: '<a>\n    <b/>\n    <c>\n        <d/>\n    </c>\n</a>',
    })
    const preserved = '<a xml:space="preserve"> <b/> </a>'
    expect(dataDocument('xml', [{ row: 1, value: preserved }])).toEqual({ text: preserved })
  })

  it('gathers several documents in <data>, without declarations nor NULLs', () => {
    expect(
      dataDocument('xml', [
        { row: 1, value: '<?xml version="1.0"?><a/>' },
        { row: 2, value: null },
        { row: 3, value: '<b><c/></b>' },
      ]),
    ).toEqual({ text: '<data>\n    <a/>\n    <b>\n        <c/>\n    </b>\n</data>' })
  })

  it('reports a malformed value, a DOCTYPE to gather and a NULL alone', () => {
    expect(dataDocument('xml', [{ row: 7, value: '<a><b></a>' }])).toEqual({
      message: 'La valeur de la ligne 7 n’est pas du XML bien formé.',
    })
    expect(
      dataDocument('xml', [
        { row: 1, value: '<a/>' },
        { row: 2, value: '<!DOCTYPE b [<!ENTITY e "x>">]><b>&e;</b>' },
      ]),
    ).toEqual({
      message:
        'La valeur de la ligne 2 déclare un DOCTYPE : elle ne peut pas être regroupée dans <data>.',
    })
    expect(dataDocument('xml', [{ row: 2, value: null }])).toEqual({
      message: 'La valeur sélectionnée est NULL.',
    })
  })
})
