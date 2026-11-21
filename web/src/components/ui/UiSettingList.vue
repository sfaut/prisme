<!--
  List of settings: its rows (`UiSettingRow`) in a frame, a line between two.
  With a title, the list is a section named by it, the title above the frame:
  the lists of a window part its settings by subject.
-->
<script setup lang="ts">
import { useId } from 'vue'

defineProps<{
  /**
   * Subject of the list, above its frame; absent, none.
   */
  title?: string
}>()

/**
 * Id of the title, which names the section.
 */
const titleId = useId()
</script>

<template>
  <component
    :is="title ? 'section' : 'div'"
    class="flex flex-col gap-2"
    :aria-labelledby="title ? titleId : undefined"
  >
    <h4 v-if="title" :id="titleId" class="text-sm font-semibold text-fg-secondary">
      {{ title }}
    </h4>
    <div class="flex flex-col divide-y divide-line rounded-lg border border-line">
      <slot />
    </div>
  </component>
</template>
