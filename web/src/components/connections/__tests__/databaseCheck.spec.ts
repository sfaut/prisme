import { describe, expect, it } from 'vitest'

import type { DatabaseState } from '@/api/generated/DatabaseState'

import type { DatabaseInput } from '../connectionForm'
import { checkOf, reviewText, stateOf } from '../databaseCheck'

/**
 * Input of the database `schema` at `location`.
 */
function input(
  schema: string,
  origin: DatabaseInput['origin'],
  location = 'D:\\data\\v.db',
): DatabaseInput {
  return { schema, origin, location }
}

/**
 * State of an open database of 2 ko, in journal `wal`.
 */
function open(schema: string, read_only = false): DatabaseState {
  return {
    schema,
    location: { scheme: 'file', address: 'D:\\data\\v.db' },
    status: 'open',
    bytes: 2048,
    journal_mode: 'wal',
    read_only,
  }
}

/**
 * State of a database whose file is missing.
 */
function missing(schema: string, scheme: 'file' | 'https' = 'file'): DatabaseState {
  return {
    schema,
    location: { scheme, address: 'D:\\data\\v.db' },
    status: 'unavailable',
    failure: { reason: 'file_not_found', location: 'D:\\data\\v.db' },
  }
}

describe('databaseCheck', () => {
  it('finds the state of a schema whatever its case', () => {
    const states = [open('main'), open('Ref')]
    expect(stateOf(states, ' ref ')?.schema).toBe('Ref')
    expect(stateOf(states, 'autre')).toBeUndefined()
  })

  it('tells an open database with its size and its journal', () => {
    expect(checkOf(input('ref', 'existing'), open('ref', true))).toEqual({
      tone: 'success',
      text: 'Ouverte : 2,0 ko, journal wal, en lecture seule.',
      fixes: [],
    })
    expect(checkOf(input('main', 'memory', ''), open('main')).text).toBe(
      'En mémoire : vide à chaque session.',
    )
  })

  it('refuses a new file that exists already', () => {
    expect(checkOf(input('ref', 'new'), open('ref'))).toMatchObject({
      tone: 'error',
      fixes: ['open', 'remove'],
    })
    expect(checkOf(input('main', 'new'), open('main')).fixes).toEqual(['open'])
  })

  it('creates a new file at saving', () => {
    expect(checkOf(input('main', 'new'), missing('main'))).toEqual({
      tone: 'info',
      text: 'Créée vide à l’enregistrement.',
      fixes: [],
    })
  })

  it('offers to create or remove a missing file', () => {
    expect(checkOf(input('main', 'existing'), missing('main'))).toEqual({
      tone: 'error',
      text: 'Introuvable. Sans elle, aucune session ne s’ouvre.',
      fixes: ['create'],
    })
    expect(checkOf(input('ref', 'existing'), missing('ref'))).toEqual({
      tone: 'warning',
      text: 'Introuvable. Les sessions s’ouvriront sans elle.',
      fixes: ['create', 'remove'],
    })
    expect(checkOf(input('ref', 'existing'), missing('ref', 'https')).fixes).toEqual(['remove'])
  })

  it('quotes any other failure', () => {
    const state: DatabaseState = {
      schema: 'ref',
      location: { scheme: 'file', address: 'D:\\data\\v.db' },
      status: 'unavailable',
      failure: {
        reason: 'not_a_database',
        location: 'D:\\data\\v.db',
        cause: { source: 'driver', message: 'file is not a database' },
      },
    }
    const check = checkOf(input('ref', 'existing'), state)
    expect(check.tone).toBe('warning')
    expect(check.text).toContain('D:\\data\\v.db n’est pas une base de données.')
    expect(check.text).toMatch(/Les sessions s’ouvriront sans elle\.$/)
    expect(check.fixes).toEqual(['remove'])
  })

  it('counts the databases to look at again', () => {
    const inputs = [input('main', 'existing'), input('ref', 'existing'), input('new', 'new')]
    expect(reviewText(inputs, [open('main'), open('ref'), missing('new')])).toBeNull()
    expect(reviewText(inputs, [open('main'), missing('ref'), missing('new')])).toBe(
      '1 base à revoir, ci-dessous.',
    )
    expect(reviewText(inputs, [missing('main'), missing('ref'), open('new')])).toBe(
      '3 bases à revoir, ci-dessous.',
    )
  })

  it('says a database was not tested', () => {
    expect(checkOf(input('ref', 'existing'), undefined)).toEqual({
      tone: 'info',
      text: 'Non testée.',
      fixes: [],
    })
  })
})
