import { fileURLToPath, URL } from 'node:url'

import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import ui from '@nuxt/ui/vite'
import vueDevTools from 'vite-plugin-vue-devtools'

// Adresse du serveur Prisme (cargo run) vers lequel le serveur de développement
// de Vite redirige l'API et les WebSockets.
const PRISME_SERVER = 'http://127.0.0.1:8484'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    vue(),
    // Nuxt UI sans routeur (une seule page). Les icônes utilisées sont
    // embarquées dans le bundle (collection @iconify-json/lucide) : aucune
    // n'est téléchargée à l'exécution. Les fenêtres et les messages
    // éphémères s'affichent en texte plus petit que le reste de l'interface.
    ui({
      router: false,
      icon: { clientBundle: { scan: true } },
      ui: {
        colors: { primary: 'blue', neutral: 'zinc' },
        modal: { slots: { title: 'text-sm', description: 'text-xs', body: 'text-xs' } },
        toast: { slots: { title: 'text-xs', description: 'text-xs' } },
      },
    }),
    vueDevTools(),
  ],
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
