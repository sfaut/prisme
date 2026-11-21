<!--
  Window of the values of a `GROUP_CONCAT` (`LIMIT` and `OFFSET`), in the
  submenu of its measure (`PivotSections.vue`). `OFFSET` only goes with a
  `LIMIT`, as MariaDB writes it. What is typed applies at Enter, on leaving a
  field and when the submenu closes, except by Escape, which forgets it
  (`useSubmenuDrafts`); a number refused shows why and changes nothing.
-->
<script setup lang="ts">
import { reactive, watch } from 'vue'

import type { PivotConcat } from './pivotConfig'
import { useSubmenuDrafts } from '@/components/ui/submenuDrafts'
import UiInput from '@/components/ui/UiInput.vue'
import { vTooltip } from '@/components/ui/tooltip'

/**
 * Largest `LIMIT` or `OFFSET`: the engine reads them on 32 bits, its largest
 * word standing for no `LIMIT`.
 */
const LARGEST = 0xffff_fffe

/**
 * Fields of the numbers of the window.
 */
const NUMBER_FIELDS = ['limit', 'offset'] as const

/**
 * Field of a number of the window.
 */
type NumberField = (typeof NUMBER_FIELDS)[number]

const props = defineProps<{
  /**
   * `LIMIT` of the measure, absent for all the values.
   */
  limit?: number
  /**
   * `OFFSET` of the measure.
   */
  offset?: number
}>()

const emit = defineEmits<{
  /**
   * The user changes the window: `LIMIT`, absent for all the values, and `OFFSET`, absent without
   * a `LIMIT`.
   */
  change: [window: Pick<PivotConcat, 'limit' | 'offset'>]
}>()

/**
 * Text of a number as its field shows it, empty without one.
 *
 * @param name Field.
 */
function numberText(name: NumberField): string {
  const value = props[name]
  return value === undefined || (name === 'offset' && value === 0) ? '' : String(value)
}

/**
 * Texts being typed.
 */
const drafts = reactive({ limit: numberText('limit'), offset: numberText('offset') })

/**
 * Text of each number when the user last confirmed it, which its refusal reads: the field does not
 * complain during the typing.
 */
const confirmed = reactive({ limit: drafts.limit, offset: drafts.offset })

watch(
  () => [props.limit, props.offset],
  () => {
    for (const name of NUMBER_FIELDS) {
      drafts[name] = numberText(name)
      confirmed[name] = drafts[name]
    }
  },
)

/**
 * Reads a number typed.
 *
 * @param text Text typed, trimmed.
 * @returns The number, `undefined` for an empty field, `null` for a refused text.
 */
function readNumber(text: string): number | undefined | null {
  if (text === '') {
    return undefined
  }
  const value = Number(text)
  return /^\d+$/.test(text) && value <= LARGEST ? value : null
}

/**
 * Why the text of a number is refused, `null` otherwise.
 *
 * @param name Field.
 */
function refusal(name: NumberField): string | null {
  return readNumber(confirmed[name]) === null
    ? `Nombre entier de 0 à ${LARGEST.toLocaleString('fr-FR')} attendu`
    : null
}

const { confirm, onKeydown } = useSubmenuDrafts(() => {
  const typed = (name: NumberField) => {
    const text = drafts[name].trim()
    drafts[name] = text
    confirmed[name] = text
    const value = readNumber(text)
    return value === null ? props[name] : value
  }
  const limit = typed('limit')
  const offset = typed('offset')
  // Without `LIMIT`, `OFFSET` leaves, as MariaDB writes it only after a `LIMIT`.
  const kept = limit === undefined || offset === 0 ? undefined : offset
  if (limit !== props.limit || kept !== (props.offset || undefined)) {
    emit('change', { limit, offset: kept })
  }
})
</script>

<template>
  <div class="flex w-64 flex-col gap-2" @keydown="onKeydown">
    <div class="flex gap-3">
      <div v-for="name in NUMBER_FIELDS" :key="name" class="flex min-w-0 flex-1 flex-col gap-1">
        <span class="text-sm font-medium text-fg-secondary" aria-hidden="true">
          {{ name.toUpperCase() }}
        </span>
        <ui-input
          v-model="drafts[name]"
          inputmode="numeric"
          :aria-label="name.toUpperCase()"
          :aria-invalid="refusal(name) !== null"
          :disabled="name === 'offset' && limit === undefined"
          :placeholder="name === 'limit' ? 'Toutes' : '0'"
          v-tooltip="
            name === 'offset' && limit === undefined
              ? { title: 'OFFSET demande un LIMIT', description: 'Comme dans MariaDB' }
              : undefined
          "
          @blur="confirm"
          @keydown.enter.prevent="confirm"
        />
      </div>
    </div>
    <p
      v-for="name in NUMBER_FIELDS.filter((field) => refusal(field) !== null)"
      :key="name"
      role="alert"
      class="text-sm text-error"
    >
      {{ name.toUpperCase() }} : {{ refusal(name) }}
    </p>
  </div>
</template>
