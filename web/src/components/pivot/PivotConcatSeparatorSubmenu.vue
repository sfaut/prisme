<!--
  Separator of a `GROUP_CONCAT` (`SEPARATOR`), in the submenu of its measure
  (`PivotSections.vue`): its field, then the usual separators (`SEPARATORS`).

  The field keeps the separator as it is typed, its spaces included, and writes
  a line break, a carriage return, a tab and a backslash with a backslash
  (`\n`, `\r`, `\t`, `\\`), as MariaDB's literals do: a field of one line holds
  no line break. What is typed applies at Enter, on leaving the field and when
  the submenu closes, except by Escape, which forgets it (`useSubmenuDrafts`).
  A usual separator applies at once.
-->
<script setup lang="ts">
import { ref, watch } from 'vue'

import { SEPARATORS } from './pivotChoices'
import { escapeSeparator, separatorLiteral, unescapeSeparator } from './pivotConfig'
import { MENU_ENTRY } from '@/components/ui/menuItem'
import { useSubmenuDrafts } from '@/components/ui/submenuDrafts'
import UiInput from '@/components/ui/UiInput.vue'

const props = defineProps<{
  /**
   * Separator of the measure.
   */
  separator: string
}>()

const emit = defineEmits<{
  /**
   * The user changes the separator.
   */
  change: [separator: string]
}>()

/**
 * Separator being typed, as its field writes it.
 */
const draft = ref(escapeSeparator(props.separator))

watch(
  () => props.separator,
  (separator) => (draft.value = escapeSeparator(separator)),
)

const { confirm, onKeydown } = useSubmenuDrafts(() => {
  const separator = unescapeSeparator(draft.value)
  if (separator !== props.separator) {
    emit('change', separator)
  }
})

/**
 * Takes a usual separator.
 *
 * @param separator Separator.
 */
function choose(separator: string): void {
  draft.value = escapeSeparator(separator)
  if (separator !== props.separator) {
    emit('change', separator)
  }
}
</script>

<template>
  <div class="flex w-72 flex-col gap-2" @keydown="onKeydown">
    <div class="flex flex-col gap-1">
      <ui-input
        v-model="draft"
        aria-label="Séparateur"
        @blur="confirm"
        @keydown.enter.prevent="confirm"
      />
      <p class="text-sm text-fg-secondary">
        \n retour à la ligne, \r retour chariot, \t tabulation
      </p>
    </div>
    <div
      class="flex flex-col border-t border-line pt-1"
      role="group"
      aria-label="Séparateurs courants"
    >
      <button
        v-for="{ separator: usual, label, icon } in SEPARATORS"
        :key="label"
        type="button"
        :aria-pressed="usual === separator"
        :data-state="usual === separator ? 'checked' : 'unchecked'"
        :class="[
          MENU_ENTRY,
          'text-fg hover:bg-surface-raised hover:text-fg-strong focus-visible:bg-surface-raised focus-visible:text-fg-strong',
        ]"
        @click="choose(usual)"
      >
        <component :is="icon" class="size-5 shrink-0" />
        <span class="min-w-0 flex-1 truncate text-start text-trim">{{ label }}</span>
        <span class="text-fg-secondary text-trim">{{ separatorLiteral(usual) }}</span>
      </button>
    </div>
  </div>
</template>
