import { fileURLToPath } from 'node:url'
import { mergeConfig, defineConfig, configDefaults } from 'vitest/config'
import viteConfig from './vite.config.ts'

export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: 'jsdom',
      // Node 25+ has its own localStorage, which warns without a storage file
      // (--localstorage-file): the tests use jsdom's, as in a browser.
      execArgv: ['--no-experimental-webstorage'],
      // Text measurements missing from jsdom, which CodeMirror needs.
      setupFiles: ['./src/testSetup.ts'],
      // One worker per core makes the tests that mount CodeMirror under jsdom
      // exceed their 5 s, for no fault of theirs; four workers take as long.
      maxWorkers: 4,
      exclude: [...configDefaults.exclude, 'e2e/**'],
      root: fileURLToPath(new URL('./', import.meta.url)),
    },
  }),
)
