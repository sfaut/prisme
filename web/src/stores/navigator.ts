/**
 * Object navigator, the panel on the left of the screen (`NavigatorPanel.vue`):
 * whether it is open, and its width. It is open by default; folded, it leaves
 * a rail on its edge, which opens it again (`PanelRail.vue`).
 *
 * Both are display preferences specific to this machine, kept in the
 * browser's local storage (`prisme.navigator.open`, `prisme.navigator.width`);
 * when storage is unavailable, the defaults apply.
 */

import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

import { loadFlag, loadSize, saveFlag, saveSize } from '@/utils/storedValues'

/**
 * Key of the state of the navigator in local storage.
 */
const OPEN_KEY = 'prisme.navigator.open'

/**
 * Key of the width of the navigator in local storage.
 */
const WIDTH_KEY = 'prisme.navigator.width'

export const useNavigatorStore = defineStore('navigator', () => {
  /**
   * Is the navigator open?
   */
  const open = ref(loadFlag(OPEN_KEY, true))

  watch(open, (value) => saveFlag(OPEN_KEY, value))

  /**
   * Width given by the user, in CSS pixels; `null`: the default width of the
   * theme.
   */
  const width = ref(loadSize(WIDTH_KEY))

  watch(width, (value) => saveSize(WIDTH_KEY, value))

  return { open, width }
})
