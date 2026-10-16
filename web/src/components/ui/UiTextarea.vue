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
    class="min-w-0 resize-y rounded-control border border-line bg-surface px-control-x py-1 font-mono text-ui text-fg-strong placeholder:text-fg-dimmed focus:border-accent focus:outline-none"
  />
</template>
