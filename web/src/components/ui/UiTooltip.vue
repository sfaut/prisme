<!--
  Bulle d'information au survol, sur `Tooltip` de Reka UI. Le déclencheur
  est l'emplacement par défaut, le texte de la bulle l'emplacement
  `content`. Demande un `TooltipProvider` plus haut (dans `App.vue`).
-->
<script setup lang="ts">
import { TooltipContent, TooltipPortal, TooltipRoot, TooltipTrigger } from 'reka-ui'

import { cssPixels } from '@/cssLength'

/**
 * Écart avec le déclencheur, en pixels (`--popup-offset` du thème).
 */
const OFFSET = cssPixels('--popup-offset', 4)
</script>

<template>
  <tooltip-root>
    <tooltip-trigger as-child>
      <slot />
    </tooltip-trigger>
    <tooltip-portal>
      <tooltip-content
        side="bottom"
        align="start"
        :side-offset="OFFSET"
        class="z-50 rounded-control border border-line bg-surface px-control-x py-1 text-ui text-fg-strong shadow-md"
      >
        <slot name="content" />
      </tooltip-content>
    </tooltip-portal>
  </tooltip-root>
</template>
