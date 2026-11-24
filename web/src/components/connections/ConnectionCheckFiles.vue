<!--
  Result of the test of a source held in files, database by database, at
  the "Vérification" step of a creation: each database with its schema, the
  protocol and the location of its file, then its state (`databaseCheck.ts`)
  and the fixes it offers. "Le créer" turns a missing file into a new one,
  created empty at saving; "L’ouvrir tel quel" turns a new file that exists
  already into an existing one; "Retirer" removes an attached database from
  the connection. Each fix changes the input in place, without a new test:
  the state of the file stays the one tested.
-->
<script setup lang="ts">
import {
  CircleAlert,
  CircleCheckBig,
  CircleX,
  FileCheck,
  FilePlus,
  Info,
  Trash2,
} from '@lucide/vue'
import { computed } from 'vue'

import type { DatabaseState } from '@/api/generated/DatabaseState'

import { type ConnectionForm, schemeOf } from './connectionForm'
import { type DatabaseFix, checkOf, stateOf } from './databaseCheck'

import ConnectionSchemeBadge from './ConnectionSchemeBadge.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiSettingList from '@/components/ui/UiSettingList.vue'

const props = defineProps<{
  /**
   * State of each database, as the test gave it.
   */
  states: DatabaseState[]
}>()

/**
 * Input, changed in place by the fixes.
 */
const form = defineModel<ConnectionForm>('form', { required: true })

/**
 * Icon and color of each tone, those of the banners (`UiAlert`).
 */
const TONES = {
  info: { classes: 'text-primary', icon: Info },
  success: { classes: 'text-success', icon: CircleCheckBig },
  warning: { classes: 'text-warning', icon: CircleAlert },
  error: { classes: 'text-error', icon: CircleX },
} as const

/**
 * Label and icon of each fix.
 */
const FIXES = {
  create: { label: 'Le créer', icon: FilePlus },
  open: { label: 'L’ouvrir tel quel', icon: FileCheck },
  remove: { label: 'Retirer', icon: Trash2 },
} as const

/**
 * Each database of the input with the result of its test, and the protocol
 * of its location.
 */
const rows = computed(() =>
  form.value.databases.map((database) => ({
    database,
    check: checkOf(database, stateOf(props.states, database.schema)),
    scheme: database.origin === 'memory' ? 'memory' : schemeOf(database.location),
  })),
)

/**
 * Applies a fix to the database at `index` of the input.
 *
 * @param index Rank of the database in the input.
 * @param fix Fix chosen.
 */
function apply(index: number, fix: DatabaseFix): void {
  const database = form.value.databases[index]!
  if (fix === 'remove') {
    form.value.databases.splice(index, 1)
  } else {
    database.origin = fix === 'create' ? 'new' : 'existing'
  }
}
</script>

<template>
  <ui-setting-list title="Bases">
    <div
      v-for="({ database, check, scheme }, index) in rows"
      :key="`${index}-${database.schema}`"
      class="flex items-start gap-3 px-4 py-3"
    >
      <component
        :is="TONES[check.tone].icon"
        class="size-5 shrink-0"
        :class="TONES[check.tone].classes"
        aria-hidden="true"
      />
      <div class="flex min-w-0 flex-1 flex-col gap-1">
        <div class="flex min-w-0 items-center gap-2">
          <span class="font-mono font-medium text-fg-strong">{{ database.schema }}</span>
          <connection-scheme-badge v-if="scheme" :scheme="scheme" />
          <span
            v-if="database.origin !== 'memory'"
            class="min-w-0 truncate font-mono text-sm text-fg-secondary"
          >
            {{ database.location }}
          </span>
        </div>
        <p class="text-sm wrap-break-word whitespace-pre-line text-fg-secondary">
          {{ check.text }}
        </p>
      </div>
      <div v-if="check.fixes.length > 0" class="flex shrink-0 items-center gap-2">
        <ui-button
          v-for="fix in check.fixes"
          :key="fix"
          :icon="FIXES[fix].icon"
          :label="FIXES[fix].label"
          small
          @click="apply(index, fix)"
        />
      </div>
    </div>
  </ui-setting-list>
</template>
