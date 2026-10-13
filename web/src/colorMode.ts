/**
 * Thème clair, sombre ou celui du système.
 *
 * Le choix est retenu dans le navigateur (`localStorage`). « Système » ne
 * pose aucune classe : les couleurs suivent alors le réglage du poste
 * (`color-scheme: light dark` dans `main.css`). « Clair » et « Sombre »
 * posent la classe `light` ou `dark` sur `<html>`, qui impose l'un ou
 * l'autre.
 */

import { ref, watch } from 'vue'

/**
 * Choix possibles.
 */
export type ColorMode = 'system' | 'light' | 'dark'

/**
 * Clef du choix dans `localStorage`.
 */
const STORAGE_KEY = 'prisme-color-mode'

/**
 * Lit le choix retenu ; « système » s'il n'y en a pas ou s'il est illisible.
 */
function storedMode(): ColorMode {
  try {
    const value = localStorage.getItem(STORAGE_KEY)
    return value === 'light' || value === 'dark' ? value : 'system'
  } catch {
    return 'system'
  }
}

/**
 * Choix en cours, partagé par toute l'application.
 */
export const colorMode = ref<ColorMode>(storedMode())

/**
 * Applique un choix à la page : classe sur `<html>`, puis le retient.
 *
 * @param mode Choix à appliquer.
 */
function apply(mode: ColorMode): void {
  const root = document.documentElement
  root.classList.toggle('light', mode === 'light')
  root.classList.toggle('dark', mode === 'dark')
  try {
    localStorage.setItem(STORAGE_KEY, mode)
  } catch {
    // Stockage refusé (navigation privée) : le choix vaut pour cette visite.
  }
}

watch(colorMode, apply, { immediate: true })
