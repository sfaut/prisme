<!--
  Row of a list of settings (`UiSettingList`): the name of the setting and,
  under it, its description, subtle, on the left; its control (default slot)
  on the right. The name is the label of the control whose `id` it is given
  (`for`). `stacked` puts the control under the text, over the whole width,
  for a control too wide for the right (a grid of check boxes).

  The description says what the setting changes, its bounds and its effects:
  what a tooltip would hide stays in sight. A `description` slot replaces the
  text, for a description with markup (a command in the code font).
-->
<script setup lang="ts">
defineProps<{
  /**
   * Name of the setting.
   */
  label: string
  /**
   * What the setting changes, under its name; absent, none.
   */
  description?: string
  /**
   * Id of the control that the name labels; absent, the name is plain text.
   */
  for?: string
  /**
   * Control under the text, over the whole width, instead of on the right.
   */
  stacked?: boolean
}>()
</script>

<template>
  <div class="flex px-4 py-3" :class="stacked ? 'flex-col gap-3' : 'items-center gap-6'">
    <div class="flex min-w-0 flex-1 flex-col gap-1">
      <label v-if="$props.for" :for="$props.for" class="font-medium text-fg-strong">
        {{ label }}
      </label>
      <span v-else class="font-medium text-fg-strong">{{ label }}</span>
      <p v-if="description || $slots.description" class="text-sm text-fg-secondary">
        <slot name="description">{{ description }}</slot>
      </p>
    </div>
    <div v-if="stacked">
      <slot />
    </div>
    <div v-else class="flex shrink-0 items-center gap-3">
      <slot />
    </div>
  </div>
</template>
