<!--
  Rail left by a folded panel on its edge of the layout: a narrow bar, along
  the whole edge, with the icon that opens the panel again and its title
  (vertical on a side, horizontal at the bottom). The whole rail is the
  button.
-->
<script setup lang="ts">
import { PanelBottomOpen, PanelLeftOpen, PanelRightOpen } from 'lucide-vue-next'

const props = defineProps<{
  /**
   * Title of the folded panel.
   */
  title: string
  /**
   * Edge of the panel: left (`start`), right (`end`) or bottom.
   */
  side: 'start' | 'end' | 'bottom'
}>()

const emit = defineEmits<{
  /**
   * The user opens the panel again.
   */
  expand: []
}>()

/**
 * Icon of the rail, by edge.
 */
const ICONS = {
  start: PanelLeftOpen,
  end: PanelRightOpen,
  bottom: PanelBottomOpen,
}
</script>

<template>
  <button
    type="button"
    class="panel-rail"
    :class="props.side"
    :aria-label="`Déplier : ${title}`"
    :title="`Déplier : ${title}`"
    @click="emit('expand')"
  >
    <component :is="ICONS[props.side]" class="size-icon shrink-0" aria-hidden="true" />
    <span class="panel-rail-title">{{ title }}</span>
  </button>
</template>

<style scoped>
.panel-rail {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  gap: var(--gap);
  padding: var(--rail-padding);
  background: var(--surface);
  color: var(--fg-dimmed);
  font-size: var(--font-size-ui);
  font-weight: 600;
  cursor: pointer;

  &:hover {
    background: var(--surface-muted);
    color: var(--fg-strong);
  }

  &:focus-visible {
    outline: var(--focus-width) solid var(--accent);
    outline-offset: calc(-1 * var(--focus-width));
  }

  /* On a side: along the whole height, the title written downwards. */
  &.start,
  &.end {
    flex-direction: column;
    inline-size: var(--rail-size);
  }

  &.start {
    border-inline-end: var(--line-width) solid var(--line);
  }

  &.end {
    border-inline-start: var(--line-width) solid var(--line);
  }

  /* At the bottom: along the whole width, the title on the line. */
  &.bottom {
    block-size: var(--rail-size);
    border-block-start: var(--line-width) solid var(--line);
  }
}

.panel-rail-title {
  white-space: nowrap;

  /* Written downwards, every character turned with the line: the
     typographic apostrophe of "d’objets" would otherwise stay upright, in
     a square of its own. */
  .start &,
  .end & {
    writing-mode: vertical-rl;
    text-orientation: sideways;
  }
}
</style>
