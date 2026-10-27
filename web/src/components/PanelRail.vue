<!--
  Rail left by a folded panel on its edge of the layout: a narrow bar, along
  the whole edge, with the icon that opens the panel again and its title
  (vertical on a side, horizontal at the bottom). The whole rail is the
  button. Its icon stands where the header of the open panel shows its fold
  button: a control's height below the panel's padding on a side; at the
  bottom, at the right end, before the panel's padding, the title at the
  left end.
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
      <component :is="ICONS[props.side]" class="size-5" aria-hidden="true" />
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
  gap: --spacing(3);
  padding: --spacing(3);
  background: var(--color-surface);
  color: var(--color-fg-faint);
  font-size: var(--text-base);
  font-weight: 600;
  cursor: pointer;

  &:hover {
    background: var(--color-surface-sunken);
    color: var(--color-fg-strong);
  }

  /* On a side: along the whole height, the title written downwards; the
     line beside is that of the splitter's handle. */
  &.start,
  &.end {
    flex-direction: column;
    inline-size: var(--rail-size);
    padding-block-start: --spacing(4);
  }

  /* At the bottom: along the whole width, the title at the left end, the
     icon at the right end; the line above is that of the splitter's
     handle. */
  &.bottom {
    flex-direction: row-reverse;
    justify-content: space-between;
    block-size: var(--rail-size);
    padding-inline: --spacing(4);
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
    block-size: --spacing(8);
  }

  .bottom & {
    inline-size: --spacing(7);
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
