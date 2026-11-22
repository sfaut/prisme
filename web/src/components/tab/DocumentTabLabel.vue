<!--
  Label of a tab, which can be renamed: a double click makes it editable,
  Enter confirms, Escape cancels, an empty name (or the default label) gives
  back the default label. Used by the editor and result tabs. The field
  keeps the size of the label's text, and its width follows the name typed,
  within the tab: renaming moves neither the text nor the tab's height.
  The field is 5 steps high, under the line height of its text: its focus
  ring, which goes beyond it, keeps its distance from the top and bottom
  edges of the tab, and stays inside the tab list, which scrolls and clips
  what goes beyond it.
-->
<script setup lang="ts">
import { nextTick, ref, useTemplateRef } from 'vue'
import { vTooltip } from '@/components/ui/tooltip'

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
    class="field-sizing-content h-5 max-w-full min-w-24 rounded-md border border-line bg-surface px-2 text-base text-fg-strong focus-visible:outline-(length:--focus-width) focus-visible:outline-offset-(--focus-offset) focus-visible:outline-primary"
    @blur="commit"
    @keydown.stop
    @keydown.enter.prevent="commit"
    @keydown.escape.prevent="editing = false"
    @mousedown.stop
  />
  <span
    v-else
    class="truncate text-trim"
    v-tooltip="{ title: 'Renommer', description: 'Double clic sur le nom' }"
    @dblclick="startEditing"
  >
    {{ name ?? defaultLabel }}
  </span>
</template>
