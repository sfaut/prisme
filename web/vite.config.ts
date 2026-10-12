import { fileURLToPath, URL } from 'node:url'

import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import vueDevTools from 'vite-plugin-vue-devtools'

// Adresse du serveur Prisme (cargo run) vers lequel le serveur de développement
// de Vite redirige l'API et les WebSockets.
const PRISME_SERVER = 'http://127.0.0.1:8484'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue(), vueDevTools()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    proxy: {
      '/api': { target: PRISME_SERVER, ws: true },
    },
  },
})
