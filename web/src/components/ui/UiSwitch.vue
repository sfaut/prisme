<!--
  On / off switch, on Reka UI's `Switch` (`role="switch"`, keyboard, state
  read by screen readers): a track, in the primary color when on, and a round
  thumb that slides to its end. It takes the `id` that a label points to
  (`UiSettingRow`), or an `aria-label`.
-->
<script setup lang="ts">
import { SwitchRoot, SwitchThumb } from 'reka-ui'

/**
 * Switch on or off.
 */
const model = defineModel<boolean>({ required: true })
</script>

<template>
  <switch-root
    v-model="model"
    class="flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full bg-surface-strong p-1 transition-colors data-[state=checked]:bg-primary disabled:cursor-default disabled:opacity-50"
  >
    <switch-thumb
      class="block size-4 rounded-full bg-surface shadow-sm transition-transform data-[state=checked]:translate-x-5"
    />
  </switch-root>
</template>
