import { describe, expect, it, vi } from 'vitest'

import { notify, toasts } from '../ui/toasts'

const logNotification = vi.fn<(notification: unknown) => void>()

vi.mock('@/api/console', () => ({
  logNotification: (notification: unknown) => logNotification(notification),
}))

describe('transient messages', () => {
  it('go to the debug log of the console too', () => {
    notify('Sélection copiée')
    notify('Échec de la copie', 'error')
    expect(toasts.value.map((toast) => toast.title)).toEqual([
      'Sélection copiée',
      'Échec de la copie',
    ])
    expect(logNotification.mock.calls).toEqual([
      [{ level: 'info', message: 'Sélection copiée' }],
      [{ level: 'error', message: 'Échec de la copie' }],
    ])
  })
})
