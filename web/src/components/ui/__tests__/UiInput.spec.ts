import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { defineComponent, ref } from 'vue'

import UiInput from '../UiInput.vue'

/**
 * Number field bound to a model, which the test reads.
 *
 * @param start Value of the model.
 */
function numberField(start: number) {
  const value = ref(start)
  const wrapper = mount(
    defineComponent({
      components: { UiInput },
      setup: () => ({ value }),
      template: '<ui-input v-model="value" type="number" />',
    }),
  )
  return { value, input: wrapper.find('input') }
}

describe('number field', () => {
  it('returns the input as a number', async () => {
    const { value, input } = numberField(1)
    await input.setValue('2.5')
    expect(value.value).toBe(2.5)
  })

  it('keeps the model and the typing while the input is not a number yet', async () => {
    const { value, input } = numberField(2)
    // The browser reads `2.`, on the way to `2.5`, as an empty field.
    await input.setValue('')
    expect(value.value).toBe(2)
    expect(input.element.value).toBe('')
  })
})
