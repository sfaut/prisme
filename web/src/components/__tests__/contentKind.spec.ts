import { describe, expect, it } from 'vitest'

import { detectContent, isWebAddress } from '../contentKind'

/**
 * Lignes d'une seule colonne.
 *
 * @param values Valeurs de la colonne.
 */
function column(...values: (string | null)[]): (string | null)[][] {
  return values.map((value) => [value])
}

describe('detectContent', () => {
  it('reconnaît une colonne de documents JSON, NULL et vides ignorés', () => {
    expect(detectContent(column('{"a": 1}', null, '', ' [1, 2] '), 0)).toBe('json')
  })

  it('reconnaît une colonne de documents XML', () => {
    expect(detectContent(column('<a><b/></a>', '<?xml version="1.0"?><r x="1"/>'), 0)).toBe('xml')
  })

  it('reconnaît une colonne d’adresses web, http ou https seulement', () => {
    expect(detectContent(column('https://example.com/a?b=1', 'http://x.fr'), 0)).toBe('url')
    expect(detectContent(column('https://example.com', 'javascript:alert(1)'), 0)).toBeNull()
    expect(isWebAddress('https://ok.fr/chemin')).toBe(true)
    expect(isWebAddress('https://pas ok.fr')).toBe(false)
    expect(isWebAddress(null)).toBe(false)
  })

  it('laisse en texte une colonne mélangée, des scalaires JSON ou du XML mal formé', () => {
    expect(detectContent(column('{"a": 1}', 'texte'), 0)).toBeNull()
    expect(detectContent(column('42', '"chaîne"'), 0)).toBeNull()
    expect(detectContent(column('<a><b></a>'), 0)).toBeNull()
    expect(detectContent(column('{pas du json}'), 0)).toBeNull()
    expect(detectContent(column(null, ''), 0)).toBeNull()
  })
})
