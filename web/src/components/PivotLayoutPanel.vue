<!--
  Layout of the pivot table, in a popover under its button in the header of the
  Pivoteur (C-07, F-34 to F-36): the form of the lines, the repetition of their
  labels, then for each axis that has levels its grand total and the subtotal of
  each level, the last one apart, which has none. Each choice applies at once and
  only lays the table out again.

  A single column of labels has nothing to repeat: in compact form, the repetition
  is inactive and says why.
-->
<script setup lang="ts">
import { ListIndentIncrease, ListTree, SlidersHorizontal, Table2 } from '@lucide/vue'
import { PopoverContent, PopoverPortal, PopoverRoot, PopoverTrigger } from 'reka-ui'
import { computed } from 'vue'

import type { PivotForm } from '@/api/generated/PivotForm'
import { cssPixels } from '@/cssLength'

import {
  type PivotAxis,
  type PivotConfig,
  hasGrandTotal,
  withAllSubtotals,
  withForm,
  withGrandTotal,
  withRepeatedLabels,
  withSubtotal,
  zoneLabels,
} from './pivotConfig'
import { MENU_PANEL } from './ui/menuItem'
import UiButton from './ui/UiButton.vue'
import UiSelect from './ui/UiSelect.vue'
import { vTooltip } from './ui/tooltip'

/**
 * Gap with the button, in pixels (`--popup-offset` of the theme).
 *
 * Fixed size: Reka UI's `side-offset` only takes pixels, so this theme
 * length is read once (`docs/DEVELOPER.md`, section "Lengths in pixels").
 */
const OFFSET = cssPixels('--popup-offset', 4)

/**
 * Forms of the lines, with their icon.
 */
const FORMS = [
  { value: 'compact' as const, label: 'Compact', icon: ListTree },
  { value: 'outline' as const, label: 'Plan', icon: ListIndentIncrease },
  { value: 'tabular' as const, label: 'Tabulaire', icon: Table2 },
]

/**
 * Axes of the table, with the name of their zone.
 */
const AXES: { axis: PivotAxis; title: string }[] = [
  { axis: 'rows', title: 'Lignes' },
  { axis: 'columns', title: 'Colonnes' },
]

const props = defineProps<{
  /**
   * Names of the columns of the result.
   */
  names: string[]
}>()

const config = defineModel<PivotConfig>('config', { required: true })

/**
 * Form of the lines.
 */
const form = computed<PivotForm>(() => config.value.form ?? 'compact')

/**
 * Takes the form chosen.
 *
 * @param chosen Form.
 */
function chooseForm(chosen: PivotForm | undefined): void {
  if (chosen !== undefined) {
    config.value = withForm(config.value, chosen)
  }
}

/**
 * Axes that have levels, each with the labels of the levels that have a subtotal.
 */
const axes = computed(() =>
  AXES.filter(({ axis }) => config.value[axis].length > 0).map(({ axis, title }) => ({
    axis,
    title,
    subtotals: zoneLabels(config.value, axis, props.names).slice(0, -1),
  })),
)

/**
 * State of a box that has just changed.
 *
 * @param event Change of the box.
 */
function ticked(event: Event): boolean {
  return (event.target as HTMLInputElement).checked
}
</script>

<template>
  <popover-root>
    <popover-trigger as-child>
      <ui-button label="Disposition" :icon="SlidersHorizontal" variant="ghost" small />
    </popover-trigger>
    <popover-portal>
      <popover-content
        align="end"
        :side-offset="OFFSET"
        :collision-padding="OFFSET"
        :class="[
          MENU_PANEL,
          'flex max-h-(--reka-popover-content-available-height) w-80 flex-col gap-3 overflow-y-auto p-3',
        ]"
        aria-label="Disposition du tableau"
      >
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium text-fg-secondary" aria-hidden="true">Mode</span>
          <ui-select
            :model-value="form"
            :items="FORMS"
            label="Mode"
            class="w-full"
            @update:model-value="chooseForm"
          />
        </div>
        <label
          class="flex items-center gap-3"
          :class="{ 'text-fg-faint': form === 'compact' }"
          v-tooltip="
            form === 'compact'
              ? 'Sans effet en mode compact : une seule colonne de libellés, rien à répéter'
              : null
          "
        >
          <input
            type="checkbox"
            class="size-5 accent-primary"
            :checked="config.repeat_labels === true"
            :disabled="form === 'compact'"
            @change="config = withRepeatedLabels(config, ticked($event))"
          />
          Répéter les libellés
        </label>
        <section
          v-for="{ axis, title, subtotals } in axes"
          :key="axis"
          class="flex flex-col gap-2 border-t border-line pt-3"
          :aria-label="title"
        >
          <h3 class="font-semibold text-fg-strong">{{ title }}</h3>
          <label class="flex items-center gap-3">
            <input
              type="checkbox"
              class="size-5 accent-primary"
              :checked="hasGrandTotal(config, axis)"
              @change="config = withGrandTotal(config, axis, ticked($event))"
            />
            Total général
          </label>
          <template v-if="subtotals.length > 0">
            <div class="flex items-center justify-between gap-2">
              <span class="text-sm font-medium text-fg-secondary">Sous-totaux</span>
              <span class="flex gap-1">
                <ui-button
                  label="Tout"
                  variant="ghost"
                  small
                  @click="config = withAllSubtotals(config, axis, true)"
                />
                <ui-button
                  label="Aucun"
                  variant="ghost"
                  small
                  @click="config = withAllSubtotals(config, axis, false)"
                />
              </span>
            </div>
            <label
              v-for="(label, level) in subtotals"
              :key="level"
              class="flex min-w-0 items-center gap-3"
            >
              <input
                type="checkbox"
                class="size-5 shrink-0 accent-primary"
                :checked="config[axis][level]?.without_subtotal !== true"
                @change="config = withSubtotal(config, axis, level, ticked($event))"
              />
              <span class="truncate">{{ label }}</span>
            </label>
          </template>
        </section>
      </popover-content>
    </popover-portal>
  </popover-root>
</template>
