<!--
  Value or bounds of a filter of the Pivoteur (F-21, F-44), in the submenu of its
  menu (`PivotSections.vue`): one field for a comparison, two for `BETWEEN`
  ("De", "À"), each with an example of the form it awaits. A field refuses a
  value that cannot compare, showing why, and changes nothing. What is typed
  applies at Enter, on leaving the field and when the submenu closes, except by
  Escape, which forgets it (`useSubmenuDrafts`).
-->
<script setup lang="ts">
import { computed, reactive, watch } from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'

import { type PivotFilter, operandCount, operandExample, operandRefusal } from './pivotConfig'
import { useSubmenuDrafts } from '@/components/ui/submenuDrafts'
import UiInput from '@/components/ui/UiInput.vue'

/**
 * Fields of the values of a test.
 */
const VALUE_FIELDS = ['operand', 'upper'] as const

/**
 * Field of a value of a test.
 */
type ValueField = (typeof VALUE_FIELDS)[number]

const props = defineProps<{
  /**
   * Filter set.
   */
  filter: PivotFilter
  /**
   * Type of each column of the result.
   */
  types: ColumnTypeInfo[]
}>()

const emit = defineEmits<{
  /**
   * The user changes the value or the bounds of the filter.
   */
  change: [changes: Partial<Record<ValueField, string>>]
}>()

/**
 * Fields the condition asks for.
 */
const fields = computed<{ name: ValueField; label: string }[]>(() =>
  operandCount(props.filter.condition) === 2
    ? [
        { name: 'operand', label: 'De' },
        { name: 'upper', label: 'À' },
      ]
    : [{ name: 'operand', label: 'Valeur' }],
)

/**
 * Text being typed in each field.
 */
const drafts = reactive({ operand: props.filter.operand, upper: props.filter.upper })

/**
 * Text of each field when the user last confirmed it, which its refusal reads: the field does not
 * complain during the typing.
 */
const confirmed = reactive({ operand: props.filter.operand, upper: props.filter.upper })

for (const name of VALUE_FIELDS) {
  watch(
    () => props.filter[name],
    (value) => {
      drafts[name] = value
      confirmed[name] = value
    },
  )
}

/**
 * Why the text of a field is refused, `null` otherwise.
 *
 * @param name Field.
 */
function refusal(name: ValueField): string | null {
  const text = confirmed[name]
  return text === '' ? null : operandRefusal(props.filter, text, props.types)
}

const { confirm, onKeydown } = useSubmenuDrafts(() => {
  const changes: Partial<Record<ValueField, string>> = {}
  for (const { name } of fields.value) {
    const text = drafts[name].trim()
    drafts[name] = text
    confirmed[name] = text
    if (refusal(name) === null && text !== props.filter[name]) {
      changes[name] = text
    }
  }
  if (Object.keys(changes).length > 0) {
    emit('change', changes)
  }
})
</script>

<template>
  <div class="flex w-64 flex-col gap-3" @keydown="onKeydown">
    <div v-for="{ name, label } in fields" :key="name" class="flex flex-col gap-1">
      <span class="text-sm font-medium text-fg-secondary" aria-hidden="true">{{ label }}</span>
      <ui-input
        v-model="drafts[name]"
        :aria-label="label"
        :aria-invalid="refusal(name) !== null"
        :placeholder="operandExample(filter, types)"
        @blur="confirm"
        @keydown.enter.prevent="confirm"
      />
      <p v-if="refusal(name)" role="alert" class="text-sm text-error">{{ refusal(name) }}</p>
    </div>
  </div>
</template>
