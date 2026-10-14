import { describe, expect, it } from 'vitest'

import { detectContent } from '../contentKind'

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

  it('laisse en texte une colonne mélangée, des scalaires JSON ou du XML mal formé', () => {
    expect(detectContent(column('{"a": 1}', 'texte'), 0)).toBeNull()
    expect(detectContent(column('42', '"chaîne"'), 0)).toBeNull()
    expect(detectContent(column('<a><b></a>'), 0)).toBeNull()
    expect(detectContent(column('{pas du json}'), 0)).toBeNull()
    expect(detectContent(column(null, ''), 0)).toBeNull()
  })
})
