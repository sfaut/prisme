<!--
  Badge of the flavor of a connection's server (`mysql`, `mariadb`): a pill
  tinted in the engine's colors, whose tooltip gives the name and the version
  of the server. Shown after the name of a source in the navigator, and of
  the connection in the connections manager.
-->
<script setup lang="ts">
import { computed } from 'vue'

import { serverName } from '@/api/flavors'
import type { Flavor } from '@/api/generated/Flavor'

import { vTooltip } from '@/components/ui/tooltip'

const props = defineProps<{
  /**
   * Flavor of the server.
   */
  flavor: Flavor
  /**
   * Version of the server (raw result of `SELECT VERSION()`), `null` if unknown.
   */
  version: string | null
}>()

/**
 * Tooltip: name and version of the server.
 */
const tooltip = computed(() => serverName(props.flavor, props.version))
</script>

<template>
  <span
    v-tooltip="tooltip"
    class="flavor-badge inline-flex shrink-0 items-center rounded-full px-1 text-xs font-medium"
    :data-flavor="flavor"
  >
    <span class="text-trim">{{ flavor }}</span>
  </span>
</template>

<style scoped>
/* Pill tinted in the engine's colors. */
.flavor-badge {
  &[data-flavor='mysql'] {
    background: var(--badge-mysql-bg);
    color: var(--badge-mysql);
  }

  &[data-flavor='mariadb'] {
    background: var(--badge-mariadb-bg);
    color: var(--badge-mariadb);
  }
}
</style>
