<!--
  Modal window, on Reka UI's `Dialog`: the focus stays in the window, the
  page behind no longer scrolls, Escape or a click outside closes it.

  Title and description at the top, closing cross, content (default slot)
  then footer (`footer` slot, for the buttons). On opening, the focus goes
  to the first input field, or failing that to the first button of the
  footer ("Annuler" of a confirmation).
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
   * Title of the window.
   */
  title: string
  /**
   * Description, under the title.
   */
  description?: string
}>()

/**
 * Window open or closed.
 */
const open = defineModel<boolean>('open', { required: true })

/**
 * Gives the focus to the first input field of the window, or failing that to
 * the first button of its footer. Without either, Reka UI gives it to the
 * first focusable element (the cross).
 *
 * @param event Reka UI's automatic focus, on opening.
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
      <dialog-overlay class="fixed inset-0 z-50 bg-overlay" />
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
