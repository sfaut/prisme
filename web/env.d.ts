/// <reference types="vite/client" />

/**
 * Path on disk of the engine's WebAssembly module, for the tests (see `pivotEngine` in
 * `vite.config.ts`).
 */
declare module 'virtual:pivot-engine-path' {
  const path: string
  export default path
}
