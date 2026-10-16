import { fileURLToPath } from 'node:url'
import { mergeConfig, defineConfig, configDefaults } from 'vitest/config'
import viteConfig from './vite.config.ts'

export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: 'jsdom',
      // Node 25+ a son propre localStorage, qui avertit faute de fichier de
      // stockage (--localstorage-file) : les tests utilisent celui de jsdom,
      // comme dans un navigateur.
      execArgv: ['--no-experimental-webstorage'],
      // Mesures du texte absentes de jsdom, dont CodeMirror a besoin.
      setupFiles: ['./src/testSetup.ts'],
      exclude: [...configDefaults.exclude, 'e2e/**'],
      root: fileURLToPath(new URL('./', import.meta.url)),
    },
  }),
)
