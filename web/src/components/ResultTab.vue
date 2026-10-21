<!--
  Content of a query's tab, in the cell's tab bar.

  - Progress dot, then label: the name given by the user, or the number and
    the start of the query; a double click renames it (`TabLabel`).
  - Pin: a pinned tab stays when the query is run again. It is always
    visible on a pinned tab; otherwise, like the cross, on hovering the tab
    or on the active tab (the tab carries the `group` class).
  - Cross (or middle click): closes the tab; the cell stops its query if
    it is running.
  - Progress bar drawn on the bottom edge of the tab: indeterminate (a
    segment that goes back and forth) when the completed share is unknown,
    green once the query has ended, red on failure (`tabFraction`).
-->
<script setup lang="ts">
import { Pin, X } from '@lucide/vue'
import { computed } from 'vue'

import { type QueryTab, tabFraction, tabProgress } from '@/api/querySession'

import ProgressDot from './ProgressDot.vue'
import TabLabel from './TabLabel.vue'
import UiButton from './ui/UiButton.vue'
import { vTooltip } from './ui/tooltip'

const props = defineProps<{
  /**
   * Query of the tab.
   */
  tab: QueryTab
  /**
   * Default label, without a given name.
   */
  defaultLabel: string
}>()

const emit = defineEmits<{
  /**
   * The user closes the tab.
   */
  close: []
  /**
   * The user pins or unpins the tab.
   */
  'toggle-pin': []
  /**
   * The user renames the tab; `null` gives back the default label.
   */
  rename: [name: string | null]
}>()

/**
 * Completed share of the query, `null` if unknown.
 */
const fraction = computed(() => tabFraction(props.tab))

/**
 * Color of the bar: red on failure, green once ended, otherwise that of
 * the action. No bar while the query waits for its turn.
 */
const barTone = computed(() => {
  const progress = tabProgress(props.tab)
  if (progress === 'waiting') {
    return null
  }
  return progress === 'error' ? 'error' : progress === 'done' ? 'success' : 'accent'
})
</script>

<template>
  <span class="flex min-w-0 items-center gap-2" @auxclick.middle.prevent="emit('close')">
    <progress-dot :tab="tab" />
    <tab-label :name="tab.name" :default-label="defaultLabel" @rename="emit('rename', $event)" />
    <!-- Pin and cross side by side, without the gap that sets the label apart. -->
    <span class="flex items-center">
      <ui-button
        :icon="Pin"
        variant="ghost"
        :tone="tab.pinned ? 'accent' : 'neutral'"
        small
        :aria-pressed="tab.pinned"
        :aria-label="tab.pinned ? 'Désépingler l’onglet' : 'Épingler l’onglet'"
        v-tooltip="
          tab.pinned
            ? 'Désépingler : l’onglet sera remplacé à la prochaine exécution'
            : 'Épingler : l’onglet reste quand la requête est relancée'
        "
        :class="
          tab.pinned ? 'visible' : 'invisible group-hover:visible group-data-[state=active]:visible'
        "
        @mousedown.stop
        @click.stop="emit('toggle-pin')"
      />
      <ui-button
        :icon="X"
        variant="ghost"
        small
        aria-label="Fermer l’onglet"
        v-tooltip="'Fermer l’onglet (arrête sa requête si elle tourne)'"
        class="invisible group-hover:visible group-data-[state=active]:visible"
        @mousedown.stop
        @click.stop="emit('close')"
      />
    </span>
    <span v-if="barTone" class="tab-progress" aria-hidden="true">
      <span
        class="tab-progress-bar"
        :class="[barTone, { indeterminate: fraction === null }]"
        :style="fraction === null ? undefined : { inlineSize: `${fraction * 100}%` }"
      />
    </span>
  </span>
</template>

<style scoped>
/* Progress bar, over the whole width of the tab's bottom edge. */
.tab-progress {
  position: absolute;
  inset-inline: 0;
  inset-block-end: 0;
  block-size: var(--tab-progress-height);
  overflow: hidden;
}

.tab-progress-bar {
  display: block;
  block-size: 100%;
  transition: inline-size 0.3s ease-out;

  &.accent {
    background: var(--accent);
  }

  &.success {
    background: var(--success);
  }

  &.error {
    background: var(--error);
  }

  /* Unknown share: a segment goes back and forth from one edge to the other. */
  &.indeterminate {
    inline-size: 30%;
    animation: sweep 1.2s ease-in-out infinite alternate;
  }
}

/* The segment takes 30 % of the tab: shifted by 70 / 30 of its width,
   it touches the right edge. */
@keyframes sweep {
  from {
    transform: translateX(0);
  }

  to {
    transform: translateX(calc(70% / 0.3));
  }
}
</style>
