<!--
  Fenêtre modale, sur `Dialog` de Reka UI : le focus reste dans la fenêtre,
  la page derrière ne défile plus, Échap ou un clic à côté la ferme.

  Titre et description en tête, croix de fermeture, contenu (emplacement
  par défaut) puis pied (emplacement `footer`, pour les boutons). À
  l'ouverture, le focus va au premier champ de saisie, ou à défaut au
  premier bouton du pied (« Annuler » d'une confirmation).
-->
<script setup lang="ts">
import { X } from 'lucide-vue-next'
import {
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from 'reka-ui'

import UiButton from './UiButton.vue'

defineProps<{
  /**
   * Titre de la fenêtre.
   */
  title: string
  /**
   * Description, sous le titre.
   */
  description?: string
}>()

/**
 * Fenêtre ouverte ou fermée.
 */
const open = defineModel<boolean>('open', { required: true })

/**
 * Donne le focus au premier champ de saisie de la fenêtre, ou à défaut au
 * premier bouton de son pied. Sans l'un ni l'autre, Reka UI le donne au
 * premier élément focalisable (la croix).
 *
 * @param event Mise au point automatique de Reka UI, à l'ouverture.
 */
function focusFirst(event: Event): void {
  const content = event.target as HTMLElement
  const target = content.querySelector<HTMLElement>(
    'input:not([disabled]), select, textarea, footer button:not([disabled])',
  )
  if (target !== null) {
    event.preventDefault()
    target.focus()
  }
}
</script>

<template>
  <dialog-root v-model:open="open">
    <dialog-portal>
      <dialog-overlay class="fixed inset-0 z-50 bg-fg-strong/40" />
      <dialog-content
        class="fixed top-1/2 left-1/2 z-50 flex w-[calc(100vw-2rem)] max-w-dialog -translate-1/2 flex-col gap-panel rounded-panel border border-line bg-surface p-panel text-ui shadow-xl"
        :aria-describedby="description ? undefined : ''"
        @open-auto-focus="focusFirst"
      >
        <header class="flex items-start gap-gap">
          <div class="flex min-w-0 flex-1 flex-col gap-1">
            <dialog-title class="text-title font-semibold text-fg-strong">{{ title }}</dialog-title>
            <dialog-description v-if="description" class="text-fg-muted">
              {{ description }}
            </dialog-description>
          </div>
          <dialog-close as-child>
            <ui-button :icon="X" variant="ghost" small aria-label="Fermer" />
          </dialog-close>
        </header>
        <slot />
        <footer v-if="$slots.footer" class="flex gap-gap">
          <slot name="footer" />
        </footer>
      </dialog-content>
    </dialog-portal>
  </dialog-root>
</template>
