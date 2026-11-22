<!--
  Steps of a creation in the connections manager, across the top of its
  right pane: each step its number in a circle, then its name, a line
  between two. The current step is filled in the primary color, a step
  passed shows a check in a primary circle, the steps to come stay subtle.
-->
<script setup lang="ts">
import { Check } from '@lucide/vue'

import { CREATION_STEPS, type CreationStep } from './connectionManager'

defineProps<{
  /**
   * Current step.
   */
  step: CreationStep
}>()
</script>

<template>
  <ol class="flex items-center gap-3" aria-label="Étapes de la création">
    <template v-for="(label, index) in CREATION_STEPS" :key="label">
      <li
        class="flex items-center gap-2 whitespace-nowrap"
        :class="index + 1 === step ? 'font-medium text-fg-strong' : 'text-fg-secondary'"
        :aria-current="index + 1 === step ? 'step' : undefined"
      >
        <span
          class="grid size-6 shrink-0 place-items-center rounded-full border text-sm font-semibold tabular-nums"
          :class="
            index + 1 === step
              ? 'border-primary bg-primary text-on-primary'
              : index + 1 < step
                ? 'border-primary text-primary'
                : 'border-line'
          "
        >
          <check v-if="index + 1 < step" class="size-4" aria-label="faite" />
          <span v-else class="text-trim">{{ index + 1 }}</span>
        </span>
        {{ label }}
      </li>
      <li
        v-if="index < CREATION_STEPS.length - 1"
        class="flex-1 border-t border-line"
        aria-hidden="true"
      />
    </template>
  </ol>
</template>
