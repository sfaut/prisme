import { describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

import { mount } from '@vue/test-utils'

import UiToaster from '../UiToaster.vue'
import { notify, showEditor, toasts } from '../toasts'

const logNotification = vi.fn<(notification: unknown) => void>()

vi.mock('@/api/console', () => ({
  logNotification: (notification: unknown) => logNotification(notification),
}))

describe('transient messages', () => {
  it('go to the debug log of the console too', () => {
    notify('Sélection copiée')
    showEditor(4)
    notify('Échec de la copie', 'error')
    showEditor(null)
    expect(toasts.value.map((toast) => toast.title)).toEqual([
      'Sélection copiée',
      'Échec de la copie',
    ])
    // Under the editor on screen, whose console shows them.
    expect(logNotification.mock.calls).toEqual([
      [{ level: 'info', message: 'Sélection copiée', editor_id: undefined }],
      [{ level: 'error', message: 'Échec de la copie', editor_id: 4 }],
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
