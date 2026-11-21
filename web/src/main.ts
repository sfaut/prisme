/**
 * Entry point of the interface: reads the display settings of the user
 * (theme, active editor, console), then mounts the root component with
 * Pinia, so that the interface starts as the user left it.
 */
import './assets/main.css'
import '@/utils/colorMode'

import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { useUserDisplayStore } from './stores/userDisplay'

const app = createApp(App)
const pinia = createPinia()

app.use(pinia)

void useUserDisplayStore(pinia)
  .load()
  .finally(() => app.mount('#app'))
