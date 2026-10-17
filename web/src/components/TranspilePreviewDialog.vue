<!--
  Preview of the transpilation: each query of the request, transpiled
  towards the dialect of the server without being run, with its report
  unfolded (`TranspilationView`). The columns of the tables are read on a
  new connection to the server: the temporary tables of the session are not
  seen there.
-->
<script setup lang="ts">
import { LoaderCircle } from 'lucide-vue-next'
import { ref, watch } from 'vue'

import { previewTranspilation } from '@/api/dialects'
import { errorMessage } from '@/api/errorMessage'
import type { TranspileRequest } from '@/api/generated/TranspileRequest'
import type { TranspiledQuery } from '@/api/generated/TranspiledQuery'

import TranspilationView from './TranspilationView.vue'
import UiAlert from './ui/UiAlert.vue'
import UiButton from './ui/UiButton.vue'
import UiDialog from './ui/UiDialog.vue'

const props = defineProps<{
  /**
   * Text, dialect, connection and current database to preview; read on
   * each opening.
   */
  request: TranspileRequest | null
}>()

/**
 * Window open or closed.
 */
const open = defineModel<boolean>('open', { required: true })

/**
 * Transpiled queries, empty as long as they are not received.
 */
const queries = ref<TranspiledQuery[]>([])

/**
 * Error of the request, `null` if none.
 */
const error = ref<string | null>(null)

/**
 * A request is running.
 */
const loading = ref(false)

// Each opening transpiles the text again: it may have changed since.
watch(open, async (isOpen) => {
  if (!isOpen || props.request === null) {
    return
  }
  queries.value = []
  error.value = null
  loading.value = true
  try {
    queries.value = await previewTranspilation(props.request)
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    loading.value = false
  }
})

/**
 * Text of a transpiled query as written in the editor.
 *
 * @param query Transpiled query, with its place in the text.
 */
function written(query: TranspiledQuery): string {
  return props.request?.sql.slice(query.start, query.end) ?? ''
}
</script>

<template>
  <ui-dialog
    v-model:open="open"
    title="Aperçu de la transpilation"
    description="Requête transpilée vers le dialecte du serveur, sans l’exécuter"
  >
    <div class="flex flex-col gap-gap">
      <div v-if="loading" role="status" class="flex items-center gap-gap text-fg-muted">
        <LoaderCircle class="size-icon animate-spin" />
        Transpilation…
      </div>
      <ui-alert v-if="error" tone="error" :title="error" />
      <p v-if="!loading && !error && queries.length === 0" class="text-fg-muted">
        Aucune requête sous le curseur.
      </p>
      <transpilation-view
        v-for="query in queries"
        :key="query.start"
        :report="query.report"
        :sql="written(query)"
        open
      />
    </div>
    <template #footer>
      <ui-button label="Fermer" class="flex-1" @click="open = false" />
    </template>
  </ui-dialog>
</template>
