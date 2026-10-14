<!--
  Champ de saisie sur plusieurs lignes, en police de code (requêtes SQL).

  Se lie par `v-model` à un texte. Les attributs (`placeholder`, `rows`,
  `maxlength`, `aria-label`...) vont au champ.
-->
<script setup lang="ts">
/**
 * Texte saisi.
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
