<!--
  Bouton de l'interface.

  Trois apparences (`variant`) : plein (`solid`), bordé (`outline`, par
  défaut) ou discret (`ghost`, sans fond : au survol, seule son icône ou son
  texte fonce) ; trois tons (`tone`) : neutre (par défaut),
  action (`accent`) ou danger (`error`). Une icône Lucide se place à gauche
  du libellé ; sans libellé, le bouton est carré. Pendant `loading`, une
  roue remplace l'icône et le bouton est inactif.

  Couleurs, hauteur, marges et arrondi viennent des variables du thème
  (`main.css`).
-->
<script setup lang="ts">
import { LoaderCircle } from 'lucide-vue-next'
import { type Component, computed } from 'vue'

const props = withDefaults(
  defineProps<{
    /**
     * Libellé ; absent, le bouton n'affiche que son icône (prévoir `aria-label`).
     */
    label?: string
    /**
     * Icône Lucide, à gauche du libellé.
     */
    icon?: Component
    /**
     * Apparence.
     */
    variant?: 'solid' | 'outline' | 'ghost'
    /**
     * Ton.
     */
    tone?: 'neutral' | 'accent' | 'error'
    /**
     * Petite taille (bouton « ⋮ » de l'explorateur).
     */
    small?: boolean
    /**
     * Action en cours : roue à la place de l'icône, bouton inactif.
     */
    loading?: boolean
    /**
     * Bouton inactif.
     */
    disabled?: boolean
    /**
     * Type HTML du bouton.
     */
    type?: 'button' | 'submit'
  }>(),
  { variant: 'outline', tone: 'neutral', type: 'button' },
)

/**
 * Classes de chaque apparence, par ton. Écrites en entier pour que
 * Tailwind les trouve dans le code.
 */
const LOOKS = {
  solid: {
    neutral: 'bg-fg-strong text-surface hover:bg-fg',
    accent: 'bg-accent text-on-accent hover:bg-accent-hover',
    error: 'bg-error text-on-accent hover:bg-error-hover',
  },
  outline: {
    neutral: 'border border-line bg-surface text-fg-strong hover:bg-surface-muted',
    accent: 'border border-accent-border text-accent hover:bg-accent-soft',
    error: 'border border-error-border text-error hover:bg-error-soft',
  },
  ghost: {
    neutral: 'text-fg-dimmed hover:text-fg-strong',
    accent: 'text-accent hover:text-accent-hover',
    error: 'text-error hover:text-error-hover',
  },
} as const

const classes = computed(() => [
  LOOKS[props.variant][props.tone],
  props.small ? 'h-control-sm' : 'h-control',
  props.label ? 'px-control-x' : props.small ? 'w-control-sm' : 'w-control',
])
</script>

<template>
  <button
    :type="type"
    :disabled="disabled || loading"
    class="inline-flex shrink-0 cursor-pointer items-center justify-center gap-1.5 rounded-control text-ui font-medium whitespace-nowrap transition-colors disabled:cursor-not-allowed disabled:opacity-50"
    :class="classes"
  >
    <LoaderCircle v-if="loading" class="size-icon shrink-0 animate-spin" />
    <component :is="icon" v-else-if="icon" class="size-icon shrink-0" />
    <span v-if="label" class="text-trim">{{ label }}</span>
  </button>
</template>
