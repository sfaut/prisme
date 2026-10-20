<!--
  Scrolling area of a tab of the console (history, debug log): the table of
  the tab (default slot), oldest elements at the top. The area stays
  scrolled to the bottom while the user does not scroll up, so that the new
  elements show. The older elements are read on request, from the button at
  the top; a failure to read shows above. Also gives the tables of the
  console their common look (`console-table` class): dense lines, like those
  of the result grid, and a header that stays visible.
-->
<script setup lang="ts" generic="T extends { id: number }">
import { nextTick, useTemplateRef, watch } from 'vue'

import type { PagedList } from '@/stores/console'

import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'

const props = defineProps<{
  /**
   * Elements of the tab.
   */
  list: PagedList<T>
  /**
   * Text shown without any element.
   */
  empty: string
}>()

/**
 * Scrolling area.
 */
const scroller = useTemplateRef<HTMLElement>('scroller')

/**
 * Is the area scrolled to the bottom, or almost?
 *
 * @param element Scrolling area.
 */
function atBottom(element: HTMLElement): boolean {
  return element.scrollHeight - element.scrollTop - element.clientHeight < element.clientHeight / 4
}

// Before the lines are drawn: stuck at the bottom, the area follows them.
watch(
  () => props.list.entries,
  (entries, previous) => {
    const element = scroller.value
    const appended = previous.length === 0 || entries[0]?.id === previous[0]?.id
    if (element !== null && appended && atBottom(element)) {
      void nextTick(() => element.scrollTo({ top: element.scrollHeight }))
    }
  },
  { flush: 'pre' },
)
</script>

<template>
  <div ref="scroller" class="min-h-0 flex-1 overflow-auto">
    <ui-alert
      v-if="list.error"
      tone="error"
      :title="list.error"
      closable
      @close="list.dismissError()"
    />
    <div v-if="list.hasOlder" class="flex justify-center py-1">
      <ui-button variant="ghost" small @click="list.loadOlder()">
        Afficher les éléments précédents
      </ui-button>
    </div>
    <p v-if="list.entries.length === 0" class="p-panel text-center text-fg-muted">{{ empty }}</p>
    <slot v-else />
  </div>
</template>

<!-- Not scoped: the tables come from the tabs, through the slot. -->
<style>
.console-table {
  inline-size: 100%;
  table-layout: fixed;
  border-collapse: collapse;
  font-size: var(--font-size-small);
  font-variant-numeric: tabular-nums;
  color: var(--fg);

  & thead {
    position: sticky;
    z-index: 1;
    inset-block-start: 0;
    background: var(--surface);
  }

  & th {
    border-block-end: var(--line-width) solid var(--line);
    color: var(--fg-muted);
    font-weight: 500;
  }

  & tbody tr {
    border-block-end: var(--line-width) solid var(--line-muted);

    &:hover {
      background: var(--surface-muted);
    }
  }

  & th,
  & td {
    block-size: var(--grid-row-height);
    padding-block: 0;
    padding-inline: var(--control-padding-x);
    overflow: hidden;
    text-align: start;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Numbers, durations: aligned on the right. */
  & .end {
    text-align: end;
  }
}
</style>
