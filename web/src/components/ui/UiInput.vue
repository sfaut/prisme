<!--
  Champ de saisie d'une ligne.

  Se lie par `v-model` à un texte ou à un nombre : avec un nombre (et
  `type="number"`), la saisie est rendue en nombre. Une icône Lucide peut se
  placer à gauche (`icon`), un contenu à droite (emplacement `trailing`,
  par exemple un bouton pour vider le champ). Les attributs (`placeholder`,
  `maxlength`, `aria-label`...) vont au champ ; `class` va à l'enveloppe.
-->
<script setup lang="ts">
import type { Component } from 'vue'

defineOptions({ inheritAttrs: false })

defineProps<{
  /**
   * Icône Lucide à gauche du texte.
   */
  icon?: Component
}>()

/**
 * Valeur saisie.
 */
const model = defineModel<string | number>({ required: true })

/**
 * Reporte la saisie dans le modèle, en nombre si le modèle en est un.
 *
 * @param event Saisie dans le champ.
 */
function onInput(event: Event): void {
  const value = (event.target as HTMLInputElement).value
  model.value = typeof model.value === 'number' ? Number(value) : value
}
</script>

<template>
  <div :class="$attrs.class" class="relative flex items-center">
    <component
      :is="icon"
      v-if="icon"
      class="pointer-events-none absolute start-2 size-icon text-fg-dimmed"
    />
    <input
      v-bind="{ ...$attrs, class: undefined }"
      :value="model"
      class="h-control w-full min-w-0 rounded-control border border-line bg-surface px-control-x text-ui text-fg-strong [appearance:textfield] placeholder:text-fg-dimmed focus:border-accent focus:outline-none"
      :class="{ 'ps-7': icon, 'pe-7': $slots.trailing }"
      @input="onInput"
    />
    <div v-if="$slots.trailing" class="absolute end-1 flex items-center">
      <slot name="trailing" />
    </div>
  </div>
</template>
