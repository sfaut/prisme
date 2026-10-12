import { describe, expect, it } from 'vitest'

import { mount } from '@vue/test-utils'

import { hashIcon, keyIcon } from '../columnIcons'

describe('columnIcons', () => {
  it('dessine la clef en SVG, à la taille du texte', () => {
    const wrapper = mount({ render: keyIcon })
    const svg = wrapper.find('svg.column-icon')
    expect(svg.attributes('width')).toBe('1em')
    expect(svg.find('title').text()).toBe('Clef primaire')
  })

  it('dessine le croisillon en SVG', () => {
    const wrapper = mount({ render: hashIcon })
    expect(wrapper.find('svg.column-icon title').text()).toBe('Clef étrangère')
  })
})
