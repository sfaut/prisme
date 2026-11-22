<!--
  Writes allowed, in the preferences (`PreferencesDialog`): one check box per
  kind of write, in a grid whose columns all have the same width. A row per
  object (data, tables, views...), its name first; each verb keeps its
  column, `CREATE`, `ALTER`, `RENAME`, `DROP`, so that the `DROP` of the
  views lines up with that of the tables, an object without a verb leaving
  its cell empty. The data come first, with their own verbs (`INSERT`,
  `UPDATE`, `DELETE`, `TRUNCATE`) in the same columns, above a line.

  Greyed out and inactive with `disabled`: while the sessions are read-only,
  the database would refuse every write.
-->
<script setup lang="ts">
import type { WriteKind } from '@/api/generated/WriteKind'
import {
  WRITE_KIND_GROUPS,
  WRITE_KIND_STATEMENTS,
  type WriteKindGroup,
  writeKindTooltip,
} from '@/api/writeKinds'

import { vTooltip } from '@/components/ui/tooltip'

defineProps<{
  /**
   * Check boxes greyed out and inactive.
   */
  disabled?: boolean
}>()

/**
 * Kinds of write allowed.
 */
const model = defineModel<WriteKind[]>({ required: true })

/**
 * Verbs of the objects, one column each, in this order.
 */
const VERBS = ['CREATE', 'ALTER', 'RENAME', 'DROP']

/**
 * Column of each verb in the grid, after that of the names: a class written
 * whole, so that Tailwind generates it.
 */
const COLUMNS = ['col-start-2', 'col-start-3', 'col-start-4', 'col-start-5']

/**
 * Parts of the grid, a line between them: the group of the data, then those
 * of the objects. The data come first in the groups of the settings
 * (`WRITE_KIND_GROUPS`, in the order of the server).
 */
const PARTS: WriteKindGroup[][] = [WRITE_KIND_GROUPS.slice(0, 1), WRITE_KIND_GROUPS.slice(1)]

/**
 * Class of the column of a kind: that of its verb for an object, its rank for
 * the data, whose verbs are their own.
 *
 * @param verb Verb of the kind (`CREATE`, `INSERT`...).
 * @param rank Rank of the kind in its group.
 */
function columnOf(verb: string, rank: number): string {
  const column = VERBS.indexOf(verb)
  return COLUMNS[column === -1 ? rank : column] ?? ''
}
</script>

<template>
  <fieldset
    :disabled="disabled"
    class="flex flex-col gap-3 disabled:opacity-50"
    aria-label="Écritures autorisées"
  >
    <template v-for="(groups, part) in PARTS" :key="part">
      <div
        class="grid grid-cols-[--spacing(39)_repeat(4,minmax(0,1fr))] items-center gap-2"
        :class="{ 'border-t border-line pt-3': part > 0 }"
      >
        <template v-for="group in groups" :key="group.label">
          <span class="col-start-1 text-fg-secondary">{{ group.label }}</span>
          <label
            v-for="({ kind, verb }, rank) in group.kinds"
            :key="kind"
            class="flex h-8 min-w-0 items-center gap-2 rounded-control border border-line px-2 font-mono text-fg-strong has-checked:border-primary/40 has-checked:bg-primary/10"
            :class="columnOf(verb, rank)"
            v-tooltip="disabled ? null : writeKindTooltip(kind)"
          >
            <input
              v-model="model"
              type="checkbox"
              :value="kind"
              :aria-label="WRITE_KIND_STATEMENTS[kind]"
              class="size-5 shrink-0 accent-primary"
            />
            {{ verb }}
          </label>
        </template>
      </div>
    </template>
  </fieldset>
</template>
