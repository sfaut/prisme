<!--
  Rail left by any folded panel (side panels, console, editor, results,
  Pivoteur): a bar of one thickness (`--rail-size`), along the whole edge,
  with the icon that opens the panel again and its title, smaller than the
  open panel's and greyed. The whole rail is the button. Its icon has the
  size of the fold button's (`size-5`, like every icon). On a side, the
  icon, then the title written downwards, both centred on the width of the
  rail, the icon at the height of the open panel's fold button: folded, the
  rail stands under the pointer that folded it. At the top or at the
  bottom, the title at the left end, the icon at the right end, before the
  panel's padding, where the open panel shows its fold button.
  What the rail adds (default slot: the state of a transaction on the
  editor) stands left of the icon, at the top or at the bottom.
-->
<script setup lang="ts">
import { PanelBottomOpen, PanelLeftOpen, PanelRightOpen, PanelTopOpen } from '@lucide/vue'
import { vTooltip } from '@/components/ui/tooltip'

const props = defineProps<{
  /**
   * Title of the folded panel.
   */
  title: string
  /**
   * Edge the panel folds towards: left (`start`), right (`end`), top (the
   * panels of an editor's view, which fold up into the rail at their top)
   * or bottom (the console).
   */
  side: 'start' | 'end' | 'top' | 'bottom'
}>()

const emit = defineEmits<{
  /**
   * The user opens the panel again.
   */
  expand: []
}>()

/**
 * Icon of the rail, by edge: its chevron points the way the panel unfolds
 * (down for the panels of an editor's view, up for the console).
 */
const ICONS = {
  start: PanelLeftOpen,
  end: PanelRightOpen,
  top: PanelTopOpen,
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
    <slot />
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

  /* On a side: along the whole height, the icon and the title written
     downwards, centred on the width of the rail; the line beside is that of
     the splitter's handle. */
  &.start,
  &.end {
    flex-direction: column;
    inline-size: var(--rail-size);
    padding-inline: 0;
    /* The top padding of a side panel, under the editors' tabs (`App.vue`). */
    padding-block-start: --spacing(3);
  }

  /* At the top or at the bottom: along the whole width, the title at the
     left end, the icon at the right end, both centred on the height of the
     rail; the line beside is that of the splitter's handle. */
  &.top,
  &.bottom {
    flex-direction: row-reverse;
    justify-content: space-between;
    block-size: var(--rail-size);
    padding-block: 0;
    padding-inline: --spacing(4);
  }
}

/* Box of the icon: the width of the small fold button, the icon at its
   center. On a side, also the height of the header's line: the icon stands
   at the height of the open panel's fold button. */
.panel-rail-icon {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  inline-size: --spacing(7);

  .start &,
  .end & {
    block-size: --spacing(8);
  }
}

.panel-rail-title {
  white-space: nowrap;

  /* At the top or at the bottom, it takes the room left of what follows. */
  .top &,
  .bottom & {
    flex: 1;
    text-align: start;
  }

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
