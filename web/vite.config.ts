import { execFileSync } from 'node:child_process'
import { join, parse } from 'node:path'
import { fileURLToPath, URL } from 'node:url'

import { type Plugin, defineConfig, normalizePath, searchForWorkspaceRoot } from 'vite'
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

/**
 * Module id under which the tests read the path of the engine's WebAssembly module, to load it
 * from disk.
 */
const ENGINE_PATH_ID = 'virtual:pivot-engine-path'

/**
 * Builds the engine of the Pivoteur (`crates/web-engine`, compiled to WebAssembly) with Cargo
 * before Vite serves the page, bundles it or runs the tests, then lets the page import the module:
 * `import engineUrl from '@pivot-engine?url'` gives its address, served by the development server
 * from Cargo's target folder, or copied into `web/dist` by the build. The tests read its path on
 * disk from `virtual:pivot-engine-path`. Cargo builds again only what changed: after a change of
 * the engine, restarting `npm run dev` is enough.
 */
function pivotEngine(): Plugin {
  const repository = fileURLToPath(new URL('..', import.meta.url))
  let modulePath = ''
  return {
    name: 'prisme-pivot-engine',
    config() {
      execFileSync(
        'cargo',
        ['build', '--release', '--package', 'web-engine', '--target', 'wasm32-unknown-unknown'],
        { cwd: repository, stdio: ['ignore', 'inherit', 'inherit'] },
      )
      const metadata = JSON.parse(
        execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps'], {
          cwd: repository,
          encoding: 'utf8',
        }),
      ) as { target_directory: string }
      const folder = join(metadata.target_directory, 'wasm32-unknown-unknown', 'release')
      modulePath = normalizePath(join(folder, 'web_engine.wasm'))
      return {
        resolve: {
          alias: [{ find: /^@pivot-engine(\?.*)?$/, replacement: `${modulePath}$1` }],
        },
        // Setting the allowed folders replaces the default one, the workspace: it is given back.
        server: { fs: { allow: [searchForWorkspaceRoot(process.cwd()), normalizePath(folder)] } },
      }
    },
    resolveId(id) {
      return id === ENGINE_PATH_ID ? `\0${ENGINE_PATH_ID}` : null
    },
    load(id) {
      return id === `\0${ENGINE_PATH_ID}` ? `export default ${JSON.stringify(modulePath)}` : null
    },
  }
}

/**
 * Name of a compiled file in kebab-case, as the other files of `assets/`: the Worker takes its name
 * from its source (`pivotWorker` gives `pivot-worker`), and the engine from its crate, whose
 * compiled file Cargo writes with underscores (`web_engine` gives `web-engine`).
 *
 * @param name Name given by the source, without its extension.
 */
function kebabCase(name: string): string {
  return name
    .replace(/([a-z0-9])([A-Z])/g, '$1-$2')
    .replaceAll('_', '-')
    .toLowerCase()
}

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    pivotEngine(),
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
  worker: {
    rolldownOptions: {
      output: {
        entryFileNames: ({ name }) => `assets/${kebabCase(name)}-[hash].js`,
        assetFileNames: ({ names }) =>
          `assets/${kebabCase(parse(names[0] ?? 'asset').name)}-[hash][extname]`,
      },
    },
  },
  server: {
    proxy: {
      '/api': { target: PRISME_SERVER, ws: true },
    },
  },
})
