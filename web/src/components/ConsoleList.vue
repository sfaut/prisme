<!--
  Scrolling area of a tab of the console (history, debug log): the table of
  the tab (default slot), oldest elements at the top. The area opens on the
  most recent elements, and stays scrolled to the bottom while the user does
  not scroll up, so that the new elements show, and goes back to them when
  the filter of the list changes. Scrolled near the top, it reads the older
  elements, and keeps those in view at the same place; a failure to read
  shows above. Also gives the tables of the console their common look
  (`console-table` class): dense lines, at the natural height of their
  text, and a header that stays visible.
-->
<script setup lang="ts" generic="T extends { id: number }">
import { nextTick, onMounted, useTemplateRef, watch } from 'vue'

import type { PagedList } from '@/stores/console'

import UiAlert from './ui/UiAlert.vue'

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
 * Are the older elements being read?
 */
let readingOlder = false

/**
 * Is the area scrolled to the bottom, or almost?
 *
 * @param element Scrolling area.
 */
function atBottom(element: HTMLElement): boolean {
  return element.scrollHeight - element.scrollTop - element.clientHeight < element.clientHeight / 4
}

/**
 * Is the area scrolled to the top, or almost?
 *
 * @param element Scrolling area.
 */
function atTop(element: HTMLElement): boolean {
  return element.scrollTop < element.clientHeight / 4
}

/**
 * Reads the older elements when the area is scrolled near its top, one
 * page at a time, then puts back the elements in view at the same distance
 * from the bottom: they stay where they were on screen.
 */
async function readOlderAtTop(): Promise<void> {
  const element = scroller.value
  if (element === null || readingOlder || !props.list.hasOlder || !atTop(element)) {
    return
  }
  readingOlder = true
  const fromBottom = element.scrollHeight - element.scrollTop
  try {
    await props.list.loadOlder()
    await nextTick()
    element.scrollTop = element.scrollHeight - fromBottom
  } finally {
    readingOlder = false
  }
}

// Shown again (another tab, console unfolded): the most recent elements.
onMounted(() => {
  const element = scroller.value
  if (element !== null) {
    element.scrollTop = element.scrollHeight
  }
})

/**
 * Changes of the filter already followed.
 */
let refilters = props.list.refilters

// Before the lines are drawn: stuck at the bottom, the area follows them,
// unless older elements are being read (`readOlderAtTop` keeps them in
// place). A new filter shows the most recent elements.
watch(
  () => props.list.entries,
  () => {
    const element = scroller.value
    const refiltered = props.list.refilters !== refilters
    refilters = props.list.refilters
    if (element !== null && !readingOlder && (refiltered || atBottom(element))) {
      void nextTick(() => element.scrollTo({ top: element.scrollHeight }))
    }
  },
  { flush: 'pre' },
)
</script>

<template>
  <div ref="scroller" class="min-h-0 flex-1 overflow-auto" @scroll="readOlderAtTop">
    <ui-alert v-if="list.error" tone="error" :title="list.error" @close="list.dismissError()" />
    <p v-if="list.entries.length === 0" class="p-4 text-center text-fg-muted">{{ empty }}</p>
    <slot v-else />
  </div>
</template>

<!-- Not scoped: the tables come from the tabs, through the slot. -->
<style>
@reference "@/assets/main.css";

.console-table {
  inline-size: 100%;
  table-layout: fixed;
  border-collapse: collapse;
  font-size: var(--text-sm);
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
    padding-block: --spacing(1);
    padding-inline: --spacing(3);
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
