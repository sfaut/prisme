/**
 * Entry point of the interface: mounts the root component with Pinia.
 */
import './assets/main.css'
import '@/utils/colorMode'

import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { forgetRekaLayouts } from './components/split/storedLayout'

forgetRekaLayouts()

const app = createApp(App)

app.use(createPinia())

app.mount('#app')
