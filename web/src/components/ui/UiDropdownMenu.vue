<!--
  Menu déroulant, sur `DropdownMenu` de Reka UI (clavier, positionnement,
  fermeture par Échap ou clic à côté, accessibilité). Le déclencheur est le
  contenu de l'emplacement par défaut (un bouton) ; chaque choix a un
  libellé, une icône Lucide et une action, ou est inactif.
-->
<script setup lang="ts">
import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuPortal,
  DropdownMenuRoot,
  DropdownMenuTrigger,
} from 'reka-ui'

import type { MenuItem } from './menuItem'

import { cssPixels } from '@/cssLength'

/**
 * Écart avec le déclencheur, en pixels (`--popup-offset` du thème).
 */
const OFFSET = cssPixels('--popup-offset', 4)

defineProps<{
  /**
   * Choix du menu.
   */
  items: MenuItem[]
}>()
</script>

<template>
  <dropdown-menu-root>
    <dropdown-menu-trigger as-child>
      <slot />
    </dropdown-menu-trigger>
    <dropdown-menu-portal>
      <dropdown-menu-content
        align="end"
        :side-offset="OFFSET"
        class="z-50 min-w-menu rounded-panel border border-line bg-surface p-1 text-ui shadow-lg"
      >
        <dropdown-menu-item
          v-for="item in items"
          :key="item.label"
          :disabled="item.disabled"
          class="flex h-control cursor-pointer items-center gap-1.5 rounded-control px-control-x outline-none select-none data-disabled:cursor-not-allowed data-disabled:opacity-50 data-highlighted:bg-surface-elevated"
          :class="item.tone === 'error' ? 'text-error' : 'text-fg data-highlighted:text-fg-strong'"
          @select="item.onSelect?.()"
        >
          <component :is="item.icon" class="size-icon shrink-0" />
          <span class="text-trim">{{ item.label }}</span>
        </dropdown-menu-item>
      </dropdown-menu-content>
    </dropdown-menu-portal>
  </dropdown-menu-root>
</template>
