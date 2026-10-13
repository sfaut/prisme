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
        :side-offset="4"
        class="z-50 min-w-40 rounded-panel border border-line bg-surface p-1 text-ui shadow-lg"
      >
        <dropdown-menu-item
          v-for="item in items"
          :key="item.label"
          :disabled="item.disabled"
          class="flex h-control cursor-pointer items-center gap-1.5 rounded-control px-control-x outline-none select-none data-disabled:cursor-not-allowed data-disabled:opacity-50 data-highlighted:bg-surface-elevated"
          :class="item.tone === 'error' ? 'text-error' : 'text-fg data-highlighted:text-fg-strong'"
          @select="item.onSelect?.()"
        >
          <component :is="item.icon" class="size-3.5 shrink-0" />
          <span class="text-trim">{{ item.label }}</span>
        </dropdown-menu-item>
      </dropdown-menu-content>
    </dropdown-menu-portal>
  </dropdown-menu-root>
</template>
