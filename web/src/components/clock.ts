/**
 * Horloge partagée, pour faire courir les durées affichées.
 *
 * Une seule minuterie pour toute l'interface, qui ne tourne que tant qu'un
 * composant l'utilise. Seuls les composants qui lisent l'horloge sont
 * redessinés à chaque tic.
 */

import { type Ref, onScopeDispose, ref } from 'vue'

/**
 * Intervalle entre deux tics, en millisecondes : les durées et les nombres
 * de lignes en cours de réception changent une fois par seconde.
 */
const TICK = 1000

/**
 * Instant présent (`performance.now()`, en millisecondes avec une partie
 * décimale, insensible aux changements d'heure du poste), rafraîchi à
 * chaque tic.
 */
const now = ref(performance.now())

/**
 * Nombre de composants qui utilisent l'horloge.
 */
let users = 0

/**
 * Minuterie en cours, absente quand personne n'utilise l'horloge.
 */
let timer: ReturnType<typeof setInterval> | undefined

/**
 * Renvoie l'horloge partagée ; elle s'arrête quand le dernier composant qui l'utilise disparaît.
 */
export function useClock(): Ref<number> {
  users++
  if (timer === undefined) {
    now.value = performance.now()
    timer = setInterval(() => (now.value = performance.now()), TICK)
  }
  onScopeDispose(() => {
    users--
    if (users === 0) {
      clearInterval(timer)
      timer = undefined
    }
  })
  return now
}
