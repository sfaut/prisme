<!--
  Form of a result's user filter: text searched in all the columns (global
  filter) or in one column (targeted filter), chosen in a list. It shows
  above the grid while the filter's button, right of the result tabs, keeps
  it expanded; its field takes the focus when it appears.
-->
<script setup lang="ts">
import { Search, X } from '@lucide/vue'
import { computed, onMounted, useTemplateRef } from 'vue'

import type { UserFilter } from '@/api/querySession'

import UiButton from './ui/UiButton.vue'
import UiInput from './ui/UiInput.vue'
import UiSelect from './ui/UiSelect.vue'

const props = defineProps<{
  /**
   * Names of the result's columns, which the filter can target.
   */
  columns: string[]
}>()

/**
 * User filter, replaced as a whole on each change.
 */
const filter = defineModel<UserFilter>({ required: true })

/**
 * Searched text.
 */
const text = computed({
  get: () => filter.value.text,
  set: (value: string | number) => {
    filter.value = { ...filter.value, text: String(value) }
  },
})

/**
 * Targeted column in the list: -1 for the global filter.
 */
const column = computed({
  get: () => filter.value.column ?? -1,
  set: (value: number | undefined) => {
    filter.value = { ...filter.value, column: value === undefined || value < 0 ? null : value }
  },
})

/**
 * Choices of the list: all the columns, then each one.
 */
const columnOptions = computed(() => [
  { label: 'Toutes les colonnes', value: -1 },
  ...props.columns.map((name, index) => ({ label: name, value: index })),
])

const form = useTemplateRef<HTMLDivElement>('form')

onMounted(() => form.value?.querySelector('input')?.focus())
</script>

<template>
  <div ref="form" class="flex items-center gap-3">
    <ui-input
      v-model="text"
      :icon="Search"
      placeholder="Filtrer"
      aria-label="Texte cherché"
      class="max-w-80"
    >
      <template v-if="text !== ''" #trailing>
        <ui-button
          :icon="X"
          variant="ghost"
          small
          aria-label="Vider le filtre"
          @click="text = ''"
        />
      </template>
    </ui-input>
    <ui-select v-model="column" :items="columnOptions" label="Colonne filtrée" class="max-w-67" />
  </div>
</template>
