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
    // Les libellés sont centrés sur leurs capitales (`text-trim`, main.css).
    ui({
      router: false,
      icon: { clientBundle: { scan: true } },
      ui: {
        colors: { primary: 'blue', neutral: 'zinc' },
        button: { slots: { label: 'text-trim' } },
        badge: { slots: { label: 'text-trim' } },
        tabs: { slots: { label: 'text-trim' } },
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
  build: {
    rolldownOptions: {
      output: {
        // Bibliothèques mises à part, un fichier par famille : chacun reste
        // sous les 500 ko, le code de Prisme, qui change à chaque version,
        // reste petit, et le navigateur garde en cache les bibliothèques qui
        // n'ont pas changé. Un groupe emporte aussi les dépendances de ses
        // modules : la priorité la plus haute se sert en premier (Vue avant
        // Nuxt UI, qui en dépend). « vendor » recueille le reste, s'il en reste.
        codeSplitting: {
          groups: [
            { name: 'vue', test: /node_modules[\\/](@vue|vue|pinia)[\\/]/, priority: 40 },
            {
              name: 'codemirror',
              test: /node_modules[\\/](@codemirror|@lezer|codemirror)[\\/]/,
              priority: 30,
            },
            { name: 'reka-ui', test: /node_modules[\\/](reka-ui|@floating-ui)[\\/]/, priority: 25 },
            { name: 'nuxt-ui', test: /node_modules[\\/]@nuxt[\\/]/, priority: 20 },
            { name: 'vendor', test: /node_modules[\\/]/, priority: 10 },
          ],
        },
      },
    },
  },
  server: {
    proxy: {
      '/api': { target: PRISME_SERVER, ws: true },
    },
  },
})
