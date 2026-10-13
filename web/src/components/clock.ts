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
 * Instant présent (`Date.now()`), rafraîchi à chaque tic.
 */
const now = ref(Date.now())

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
    now.value = Date.now()
    timer = setInterval(() => (now.value = Date.now()), TICK)
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
