<!--
  Liste de choix, sur `Select` de Reka UI (clavier, positionnement de la
  liste, accessibilité). Chaque choix a un libellé, une valeur et, au
  besoin, une icône Lucide, reprise dans le bouton pour le choix en cours.
-->
<script setup lang="ts" generic="T extends string | number">
import { Check, ChevronDown } from 'lucide-vue-next'
import {
  SelectContent,
  SelectIcon,
  SelectItem,
  SelectItemIndicator,
  SelectItemText,
  SelectPortal,
  SelectRoot,
  SelectTrigger,
  SelectViewport,
} from 'reka-ui'
import { type Component, computed } from 'vue'

const props = defineProps<{
  /**
   * Choix proposés.
   */
  items: { label: string; value: T; icon?: Component }[]
  /**
   * Nom du champ, lu par les lecteurs d'écran (`aria-label`).
   */
  label: string
}>()

/**
 * Valeur choisie ; absente, le bouton reste vide.
 */
const model = defineModel<T | undefined>()

/**
 * Choix en cours, pour son libellé et son icône dans le bouton.
 */
const current = computed(() => props.items.find((item) => item.value === model.value))
</script>

<template>
  <select-root v-model="model">
    <select-trigger
      :aria-label="label"
      class="inline-flex h-control cursor-pointer items-center gap-1.5 rounded-control border border-line bg-surface px-control-x text-ui text-fg-strong hover:bg-surface-muted"
    >
      <component :is="current.icon" v-if="current?.icon" class="size-3.5 shrink-0" />
      <span class="min-w-0 flex-1 truncate text-start text-trim">{{ current?.label }}</span>
      <select-icon as-child>
        <ChevronDown class="size-3.5 shrink-0 text-fg-dimmed" />
      </select-icon>
    </select-trigger>
    <select-portal>
      <select-content
        position="popper"
        :side-offset="4"
        class="z-50 max-h-(--reka-select-content-available-height) min-w-(--reka-select-trigger-width) overflow-hidden rounded-panel border border-line bg-surface p-1 text-ui shadow-lg"
      >
        <select-viewport>
          <select-item
            v-for="item in items"
            :key="item.value"
            :value="item.value"
            class="flex h-control cursor-pointer items-center gap-1.5 rounded-control px-control-x text-fg outline-none select-none data-highlighted:bg-surface-elevated data-highlighted:text-fg-strong"
          >
            <component :is="item.icon" v-if="item.icon" class="size-3.5 shrink-0" />
            <select-item-text class="flex-1 text-trim">{{ item.label }}</select-item-text>
            <select-item-indicator>
              <Check class="size-3.5 text-accent" />
            </select-item-indicator>
          </select-item>
        </select-viewport>
      </select-content>
    </select-portal>
  </select-root>
</template>
