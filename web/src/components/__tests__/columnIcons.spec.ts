import { describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'

import { keyIcon } from '../columnIcons'

describe('columnIcons', () => {
  it('dessine la clef en SVG, à la taille du texte', () => {
    const wrapper = mount({ render: keyIcon })
    const svg = wrapper.find('svg.column-icon')
    expect(svg.attributes('width')).toBe('1em')
    expect(svg.find('title').text()).toBe('Clef primaire')
  })
})
