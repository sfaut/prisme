import { describe, expect, it } from 'vitest'

import { errorMessage } from '../errorMessage'

describe('errorMessage', () => {
  it('shows the message of an Error', () => {
    expect(errorMessage(new Error('connection refused'))).toBe('connection refused')
  })

  it('shows any other value as it is', () => {
    expect(errorMessage('plain text')).toBe('plain text')
    expect(errorMessage(42)).toBe('42')
  })
})
