<!--
  Protocol of a location, as a pill beside it: `file`, `https`, `memory`...
  The same pill shows wherever a database held in files appears, so that
  where it is read from stays in sight: a local file, a remote copy, a
  database in memory.
-->
<script setup lang="ts">
defineProps<{
  /**
   * Protocol to show, as the server reads it or as typed.
   */
  scheme: string
}>()
</script>

<template>
  <span class="shrink-0 rounded-full bg-fg-secondary/15 px-2 font-mono text-xs text-fg-secondary">
    <span class="text-trim">{{ scheme }}</span>
  </span>
</template>
