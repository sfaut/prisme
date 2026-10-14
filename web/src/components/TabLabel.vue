<!--
  Libellé d'un onglet, renommable : un double clic le rend modifiable,
  Entrée valide, Échap annule, un nom vide (ou le libellé par défaut) rend
  le libellé par défaut. Sert aux onglets d'éditeur et de résultat.
-->
<script setup lang="ts">
import { nextTick, ref, useTemplateRef } from 'vue'

const props = defineProps<{
  /**
   * Nom donné par l'utilisateur, `null` pour le libellé par défaut.
   */
  name: string | null
  /**
   * Libellé par défaut, sans nom donné.
   */
  defaultLabel: string
}>()

const emit = defineEmits<{
  /**
   * L'utilisateur renomme l'onglet ; `null` rend le libellé par défaut.
   */
  rename: [name: string | null]
}>()

/**
 * Libellé en cours de modification ?
 */
const editing = ref(false)

/**
 * Saisie du nouveau nom.
 */
const draft = ref('')

const input = useTemplateRef<HTMLInputElement>('input')

/**
 * Rend le libellé modifiable, son texte sélectionné.
 */
async function startEditing(): Promise<void> {
  draft.value = props.name ?? props.defaultLabel
  editing.value = true
  await nextTick()
  input.value?.select()
}

/**
 * Valide le nouveau nom ; vide, l'onglet reprend son libellé par défaut.
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
