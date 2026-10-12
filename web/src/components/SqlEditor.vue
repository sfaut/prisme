<!--
  Éditeur SQL, basé sur CodeMirror 6.

  Coloration selon le dialecte du serveur (MySQL ou MariaDB), numéros de
  ligne, historique d'annulation. Ctrl-Entrée (Cmd-Entrée sur Mac) émet `run`
  avec la position du curseur ; Ctrl-Maj-Entrée émet `run-all`. Le texte se
  lie avec `v-model`.
-->
<script setup lang="ts">
import { MariaSQL, MySQL, sql } from '@codemirror/lang-sql'
import { Compartment } from '@codemirror/state'
import { keymap } from '@codemirror/view'
import { EditorView, basicSetup } from 'codemirror'
import { onBeforeUnmount, onMounted, useTemplateRef, watch } from 'vue'

import type { Flavor } from '@/api/generated/Flavor'

const props = defineProps<{
  /**
   * Dialecte du serveur cible, pour la coloration et les mots-clefs.
   */
  flavor: Flavor
}>()

/**
 * Texte de l'éditeur.
 */
const text = defineModel<string>({ required: true })

const emit = defineEmits<{
  /**
   * L'utilisateur demande l'exécution de la requête sous le curseur (Ctrl-Entrée).
   */
  run: [cursor: number]
  /**
   * L'utilisateur demande l'exécution de toutes les requêtes (Ctrl-Maj-Entrée).
   */
  'run-all': []
}>()

const host = useTemplateRef<HTMLDivElement>('host')

/**
 * Emplacement de la langue dans la configuration, pour la changer avec le dialecte.
 */
const language = new Compartment()

let view: EditorView | null = null

/**
 * Langue CodeMirror du dialecte.
 *
 * @param flavor Variante du serveur.
 */
function languageOf(flavor: Flavor) {
  return sql({ dialect: flavor === 'mariadb' ? MariaSQL : MySQL })
}

onMounted(() => {
  view = new EditorView({
    parent: host.value!,
    doc: text.value,
    extensions: [
      // Avant basicSetup, pour passer devant ses propres raccourcis.
      keymap.of([
        {
          key: 'Mod-Enter',
          run: (editor) => {
            emit('run', editor.state.selection.main.head)
            return true
          },
        },
        {
          key: 'Mod-Shift-Enter',
          run: () => {
            emit('run-all')
            return true
          },
        },
      ]),
      basicSetup,
      language.of(languageOf(props.flavor)),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          text.value = update.state.doc.toString()
        }
      }),
    ],
  })
})

// Texte modifié de l'extérieur (et non par la frappe) : on remplace le contenu.
watch(text, (value) => {
  if (view && value !== view.state.doc.toString()) {
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } })
  }
})

watch(
  () => props.flavor,
  (flavor) => view?.dispatch({ effects: language.reconfigure(languageOf(flavor)) }),
)

onBeforeUnmount(() => view?.destroy())

/**
 * Position du curseur dans le texte, en unités UTF-16 (0 avant le montage).
 */
function cursor(): number {
  return view?.state.selection.main.head ?? 0
}

defineExpose({ cursor })
</script>

<template>
  <div ref="host" class="sql-editor" />
</template>

<style scoped>
.sql-editor {
  border: 1px solid var(--border-color, #ccc);
  border-radius: 4px;
  overflow: hidden;
}

.sql-editor :deep(.cm-editor) {
  min-height: 8rem;
  max-height: 24rem;
  font-size: 0.9rem;
}

.sql-editor :deep(.cm-scroller) {
  font-family: ui-monospace, 'Cascadia Code', Consolas, monospace;
}
</style>
