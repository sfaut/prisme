/**
 * Entry point of the interface: mounts the root component with Pinia.
 */
// TEMPORARY (`utils/storage.ts`): first, so that the keys of the local storage
// are moved before any module reads them (`utils/colorMode.ts` does on import).
import './utils/storage'
import './assets/main.css'
import '@/utils/colorMode'

import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'

const app = createApp(App)

app.use(createPinia())

app.mount('#app')
