<!--
  Badge of the transaction open on the session of an editor: at the right
  end of the editor's header, at the height of its buttons, or of its rail
  when the editor is folded, smaller (`small`), until its COMMIT or
  ROLLBACK (`EditorView.vue`).
-->
<script setup lang="ts">
import { vTooltip } from '@/components/ui/tooltip'

defineProps<{
  /**
   * Smaller, on the rail of the folded editor: two steps lower than the
   * controls, within the thickness of the rail.
   */
  small?: boolean
}>()

/**
 * Tooltip of the badge.
 */
const TIP =
  'Une transaction est ouverte sur la session : COMMIT la valide, ROLLBACK l’annule. Fermer la session l’annule.'
</script>

<template>
  <span
    v-tooltip="TIP"
    role="status"
    class="inline-flex shrink-0 items-center rounded-full bg-info/14 text-base font-medium whitespace-nowrap text-info"
    :class="small ? 'h-6 px-3' : 'h-8 px-4'"
  >
    <span class="text-trim">Transaction ouverte</span>
  </span>
</template>
