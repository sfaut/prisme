import { fileURLToPath, URL } from 'node:url'

import { type Plugin, defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'
import vueDevTools from 'vite-plugin-vue-devtools'

// Address of the Prisme server (cargo run) to which Vite's development server
// forwards the API and the WebSockets.
const PRISME_SERVER = 'http://127.0.0.1:8484'

/**
 * Local date and time in ISO 8601 format, to the second, with the time zone
 * offset: "2026-10-14T09:12:34+02:00".
 *
 * @param date Instant to format.
 */
function localIsoDateTime(date: Date): string {
  const pad = (value: number) => String(value).padStart(2, '0')
  const offset = -date.getTimezoneOffset()
  const sign = offset >= 0 ? '+' : '-'
  const hours = pad(Math.floor(Math.abs(offset) / 60))
  const minutes = pad(Math.abs(offset) % 60)
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
    `T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}` +
    `${sign}${hours}:${minutes}`
  )
}

/**
 * Adds the date and time of the build to `index.html`, in
 * `<meta name="build-time">`: it tells when the served interface was built
 * (Ctrl+U, or the browser's developer tools). On build only: the development
 * server has none.
 */
function buildTime(): Plugin {
  return {
    name: 'prisme-build-time',
    apply: 'build',
    transformIndexHtml: () => [
      {
        tag: 'meta',
        attrs: { name: 'build-time', content: localIsoDateTime(new Date()) },
        injectTo: 'head',
      },
    ],
  }
}

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    vue(),
    buildTime(),
    tailwindcss(),
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
        // Libraries set apart, one file per family: each stays under 500 kB,
        // Prisme's code, which changes with each version, stays small, and the
        // browser keeps in cache the libraries that did not change. A group
        // also takes the dependencies of its modules: the highest priority
        // serves first (Vue before Reka UI, which depends on it). "vendor"
        // collects the rest, if any.
        codeSplitting: {
          groups: [
            { name: 'vue', test: /node_modules[\\/](@vue|vue|pinia)[\\/]/, priority: 40 },
            {
              name: 'codemirror',
              test: /node_modules[\\/](@codemirror|@lezer|codemirror)[\\/]/,
              priority: 30,
            },
            { name: 'reka-ui', test: /node_modules[\\/](reka-ui|@floating-ui)[\\/]/, priority: 25 },
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
