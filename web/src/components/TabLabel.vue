<!--
  Label of a tab, which can be renamed: a double click makes it editable,
  Enter confirms, Escape cancels, an empty name (or the default label) gives
  back the default label. Used by the editor and result tabs.
-->
<script setup lang="ts">
import { nextTick, ref, useTemplateRef } from 'vue'

const props = defineProps<{
  /**
   * Name given by the user, `null` for the default label.
   */
  name: string | null
  /**
   * Default label, without a given name.
   */
  defaultLabel: string
}>()

const emit = defineEmits<{
  /**
   * The user renames the tab; `null` gives back the default label.
   */
  rename: [name: string | null]
}>()

/**
 * Is the label being edited?
 */
const editing = ref(false)

/**
 * Input of the new name.
 */
const draft = ref('')

const input = useTemplateRef<HTMLInputElement>('input')

/**
 * Makes the label editable, its text selected.
 */
async function startEditing(): Promise<void> {
  draft.value = props.name ?? props.defaultLabel
  editing.value = true
  await nextTick()
  input.value?.select()
}

/**
 * Confirms the new name; when empty, the tab takes back its default label.
 */
function commit(): void {
  if (editing.value) {
    editing.value = false
    const name = draft.value.trim()
    emit('rename', name === '' || name === props.defaultLabel ? null : name)
  }
}
</script>

<template>
  <input
    v-if="editing"
    ref="input"
    v-model="draft"
    aria-label="Nom de l'onglet"
    class="h-control-sm w-40 min-w-0 rounded-control border border-accent bg-surface px-1 text-ui text-fg-strong outline-none"
    @blur="commit"
    @keydown.stop
    @keydown.enter.prevent="commit"
    @keydown.escape.prevent="editing = false"
    @mousedown.stop
  />
  <span
    v-else
    class="truncate text-trim"
    title="Double-cliquer pour renommer"
    @dblclick="startEditing"
  >
    {{ name ?? defaultLabel }}
  </span>
</template>
