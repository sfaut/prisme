<!--
  Multi-line input field, in the code font (SQL queries).

  Binds with `v-model` to a text. The attributes (`placeholder`, `rows`,
  `maxlength`, `aria-label`...) go to the field.
-->
<script setup lang="ts">
/**
 * Typed text.
 */
const model = defineModel<string>({ required: true })
</script>

<template>
  <textarea
    v-model="model"
    spellcheck="false"
    class="min-w-0 resize-y rounded-lg border border-line bg-surface px-3 py-1 font-mono text-base text-fg-strong placeholder:text-fg-dimmed focus-visible:outline-(length:--focus-width) focus-visible:outline-offset-(--focus-offset) focus-visible:outline-focus-ring"
  />
</template>
