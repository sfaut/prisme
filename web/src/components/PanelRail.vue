<!--
  Rail left by a folded panel on its edge of the layout: a narrow bar, along
  the whole edge, with the icon that opens the panel again and its title
  (vertical on a side, horizontal at the bottom). The whole rail is the
  button. Its icon stands where the header of the open panel shows its fold
  button: a control's height below the panel's padding on a side, after
  the panel's padding at the bottom.
-->
<script setup lang="ts">
import { PanelBottomOpen, PanelLeftOpen, PanelRightOpen } from '@lucide/vue'
import { vTooltip } from './ui/tooltip'

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
    v-tooltip="`Déplier : ${title}`"
    @click="emit('expand')"
  >
    <span class="panel-rail-icon">
      <component :is="ICONS[props.side]" class="size-icon" aria-hidden="true" />
    </span>
    <span class="panel-rail-title">{{ title }}</span>
  </button>
</template>

<style scoped>
@reference "@/assets/main.css";

.panel-rail {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  gap: --spacing(2);
  padding: --spacing(2);
  background: var(--surface);
  color: var(--fg-dimmed);
  font-size: var(--font-size-ui);
  font-weight: 600;
  cursor: pointer;

  &:hover {
    background: var(--surface-muted);
    color: var(--fg-strong);
  }

  /* The common ring, drawn inside: the rail touches the window's edges,
     which would cut it outside. */
  &:focus-visible {
    outline-offset: calc(-1 * var(--focus-width));
  }

  /* On a side: along the whole height, the title written downwards. */
  &.start,
  &.end {
    flex-direction: column;
    inline-size: --spacing(7);
    padding-block-start: --spacing(3);
  }

  &.start {
    border-inline-end: var(--line-width) solid var(--line);
  }

  &.end {
    border-inline-start: var(--line-width) solid var(--line);
  }

  /* At the bottom: along the whole width, the title on the line. */
  &.bottom {
    block-size: --spacing(7);
    padding-inline-start: --spacing(3);
    border-block-start: var(--line-width) solid var(--line);
  }
}

/* Box of the icon, the size of the header's line (side) or of the small
   fold button (bottom), the icon at its center. */
.panel-rail-icon {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;

  .start &,
  .end & {
    block-size: var(--control-height);
  }

  .bottom & {
    inline-size: var(--control-height-sm);
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
