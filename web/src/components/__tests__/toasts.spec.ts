import { describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

import { mount } from '@vue/test-utils'

import UiToaster from '../ui/UiToaster.vue'
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

  it('offer their action, which closes them', async () => {
    toasts.value = []
    const run = vi.fn<() => void>()
    const wrapper = mount(UiToaster, { attachTo: document.body })
    notify('Export prêt', 'success', {
      label: 'Télécharger',
      alternative: 'Télécharger depuis le gestionnaire d’exports',
      run,
    })
    await nextTick()
    const button = [...document.querySelectorAll('button')].find(
      (element) => element.textContent?.trim() === 'Télécharger',
    )
    button?.click()
    await nextTick()
    expect(run).toHaveBeenCalledOnce()
    expect(toasts.value).toEqual([])
    wrapper.unmount()
  })
})
