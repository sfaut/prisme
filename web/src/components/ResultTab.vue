<!--
  Contenu de l'onglet d'une requête, dans la barre des onglets de la cellule.

  - Pastille d'avancement, puis libellé : le nom donné par l'utilisateur, ou
    le numéro et le début de la requête. Un double clic sur le libellé le
    rend modifiable : Entrée valide, Échap annule, un nom vide rend le
    libellé par défaut.
  - Épingle : un onglet épinglé reste quand la requête est relancée. Elle
    est toujours visible sur un onglet épinglé ; sinon, comme la croix, au
    survol de l'onglet ou sur l'onglet actif (l'onglet porte la classe
    `group`).
  - Croix (ou clic du milieu) : ferme l'onglet ; la cellule arrête sa
    requête si elle tourne.
  - Barre d'avancement dessinée sur le bord bas de l'onglet : indéterminée
    (un segment qui va et vient) quand la part accomplie est inconnue, verte
    une fois la requête terminée, rouge en cas d'échec (`tabFraction`).
-->
<script setup lang="ts">
import { Pin, X } from 'lucide-vue-next'
import { computed, nextTick, ref, useTemplateRef } from 'vue'

import { type QueryTab, tabFraction, tabProgress } from '@/api/querySession'

import ProgressDot from './ProgressDot.vue'
import UiButton from './ui/UiButton.vue'

const props = defineProps<{
  /**
   * Requête de l'onglet.
   */
  tab: QueryTab
  /**
   * Libellé par défaut, sans nom donné.
   */
  defaultLabel: string
}>()

const emit = defineEmits<{
  /**
   * L'utilisateur ferme l'onglet.
   */
  close: []
  /**
   * L'utilisateur épingle ou désépingle l'onglet.
   */
  'toggle-pin': []
  /**
   * L'utilisateur renomme l'onglet ; `null` rend le libellé par défaut.
   */
  rename: [name: string | null]
}>()

/**
 * Libellé en cours de modification ?
 */
const editing = ref(false)

/**
 * Saisie du nouveau nom.
 */
const draft = ref('')

const input = useTemplateRef<HTMLInputElement>('input')

/**
 * Part accomplie de la requête, `null` si inconnue.
 */
const fraction = computed(() => tabFraction(props.tab))

/**
 * Couleur de la barre : rouge en échec, verte une fois terminée, sinon
 * celle de l'action. Aucune barre tant que la requête attend son tour.
 */
const barTone = computed(() => {
  const progress = tabProgress(props.tab)
  if (progress === 'waiting') {
    return null
  }
  return progress === 'error' ? 'error' : progress === 'done' ? 'success' : 'accent'
})

/**
 * Rend le libellé modifiable, son texte sélectionné.
 */
async function startEditing(): Promise<void> {
  draft.value = props.tab.name ?? props.defaultLabel
  editing.value = true
  await nextTick()
  input.value?.select()
}

/**
 * Valide le nouveau nom ; vide, l'onglet reprend son libellé par défaut.
 */
function commit(): void {
  if (editing.value) {
    editing.value = false
    const name = draft.value.trim()
    emit('rename', name === '' || name === props.defaultLabel ? null : name)
  }
}
</script>

<template>
  <span class="flex min-w-0 items-center gap-1.5" @auxclick.middle.prevent="emit('close')">
    <progress-dot :tab="tab" />
    <input
      v-if="editing"
      ref="input"
      v-model="draft"
      aria-label="Nom de l'onglet"
      class="h-control-sm w-40 min-w-0 rounded-control border border-accent bg-surface px-1 text-ui text-fg-strong outline-none"
      @blur="commit"
      @keydown.stop
      @keydown.enter.prevent="commit"
      @keydown.escape.prevent="editing = false"
      @mousedown.stop
    />
    <span
      v-else
      class="truncate text-trim"
      title="Double-cliquer pour renommer"
      @dblclick="startEditing"
    >
      {{ tab.name ?? defaultLabel }}
    </span>
    <ui-button
      :icon="Pin"
      variant="ghost"
      :tone="tab.pinned ? 'accent' : 'neutral'"
      small
      :aria-pressed="tab.pinned"
      :aria-label="tab.pinned ? 'Désépingler l’onglet' : 'Épingler l’onglet'"
      :title="
        tab.pinned
          ? 'Désépingler : l’onglet sera remplacé à la prochaine exécution'
          : 'Épingler : l’onglet reste quand la requête est relancée'
      "
      :class="
        tab.pinned ? 'visible' : 'invisible group-hover:visible group-data-[state=active]:visible'
      "
      @mousedown.stop
      @click.stop="emit('toggle-pin')"
    />
    <ui-button
      :icon="X"
      variant="ghost"
      small
      aria-label="Fermer l’onglet"
      title="Fermer l’onglet (arrête sa requête si elle tourne)"
      class="invisible group-hover:visible group-data-[state=active]:visible"
      @mousedown.stop
      @click.stop="emit('close')"
    />
    <span v-if="barTone" class="tab-progress" aria-hidden="true">
      <span
        class="tab-progress-bar"
        :class="[barTone, { indeterminate: fraction === null }]"
        :style="fraction === null ? undefined : { inlineSize: `${fraction * 100}%` }"
      />
    </span>
  </span>
</template>

<style scoped>
/* Barre d'avancement, sur toute la largeur du bord bas de l'onglet. */
.tab-progress {
  position: absolute;
  inset-inline: 0;
  inset-block-end: 0;
  block-size: var(--tab-progress-height);
  overflow: hidden;
}

.tab-progress-bar {
  display: block;
  block-size: 100%;
  transition: inline-size 0.3s ease-out;

  &.accent {
    background: var(--accent);
  }

  &.success {
    background: var(--success);
  }

  &.error {
    background: var(--error);
  }

  /* Part inconnue : un segment va et vient d'un bord à l'autre. */
  &.indeterminate {
    inline-size: 30%;
    animation: sweep 1.2s ease-in-out infinite alternate;
  }
}

/* Le segment occupe 30 % de l'onglet : décalé de 70 / 30 de sa largeur,
   il touche le bord droit. */
@keyframes sweep {
  from {
    transform: translateX(0);
  }

  to {
    transform: translateX(calc(70% / 0.3));
  }
}
</style>
