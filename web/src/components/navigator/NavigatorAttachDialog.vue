<!--
  Window "Attacher une base", asked by the "⋮" menu of a source that
  attaches databases (SQLite) in the navigator: the origin of the database
  (an existing file, or a database in memory), its schema and its location.
  "Pour cette session" attaches it to the editor's session until it ends;
  "Dans la connexion" also adds it to the connection, which attaches it to
  every session, without closing this one. A database refused (schema taken,
  file missing) shows as an error of the session.

  Open while a source waits for its database (`v-model:source`, its name);
  confirmed, it emits `attach` and closes.
-->
<script setup lang="ts">
import { FileCheck, MemoryStick } from '@lucide/vue'
import { computed, ref, watch } from 'vue'

import { LENGTHS, MAIN_SCHEMA, schemeOf } from '@/components/connections/connectionForm'
import ConnectionSchemeBadge from '@/components/connections/ConnectionSchemeBadge.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiDialog from '@/components/ui/UiDialog.vue'
import UiField from '@/components/ui/UiField.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiSelect from '@/components/ui/UiSelect.vue'

/**
 * Origin of the database attached: a file that exists, or memory.
 */
type AttachOrigin = 'existing' | 'memory'

/**
 * Origins offered.
 */
const ORIGINS = [
  { value: 'existing' as const, label: 'Fichier existant', icon: FileCheck },
  { value: 'memory' as const, label: 'En mémoire', icon: MemoryStick },
]

/**
 * What the window says under the fields, by origin.
 */
const HINTS: Record<AttachOrigin, string> = {
  existing: 'Chemin absolu sur la machine du serveur de Prisme, ou adresse https.',
  memory: 'Vide, partagée par les requêtes de la session.',
}

/**
 * Name of the source that waits for a database, `null` if none: the window
 * is open while there is one.
 */
const source = defineModel<string | null>('source', { required: true })

const emit = defineEmits<{
  /**
   * The user attaches the database: to the session, and with `save` to the
   * connection too.
   */
  attach: [schema: string, location: string, save: boolean]
}>()

/**
 * Origin chosen.
 */
const origin = ref<AttachOrigin>('existing')

/**
 * Schema typed.
 */
const schema = ref('')

/**
 * Location typed, for an existing file.
 */
const location = ref('')

// Each opening starts from empty fields.
watch(source, () => {
  origin.value = 'existing'
  schema.value = ''
  location.value = ''
})

/**
 * Is the window open?
 */
const open = computed({
  get: () => source.value !== null,
  set: (isOpen) => {
    if (!isOpen) {
      source.value = null
    }
  },
})

/**
 * Protocol of the location typed, `null` while it says none.
 */
const scheme = computed(() => schemeOf(location.value))

/**
 * Why the database cannot be attached yet, `null` if it can: a schema
 * missing or reserved, a location missing.
 */
const blocker = computed(() => {
  const name = schema.value.trim().toLowerCase()
  if (name === '') {
    return 'schema'
  }
  if (name === MAIN_SCHEMA || name === 'temp') {
    return 'reserved'
  }
  return origin.value === 'existing' && location.value.trim() === '' ? 'location' : null
})

/**
 * Attaches the database, and closes the window.
 *
 * @param save Is it also added to the connection?
 */
function attach(save: boolean): void {
  if (blocker.value !== null) {
    return
  }
  const typed = origin.value === 'memory' ? ':memory:' : location.value.trim()
  source.value = null
  emit('attach', schema.value.trim(), typed, save)
}
</script>

<template>
  <ui-dialog
    v-model:open="open"
    title="Attacher une base"
    :description="`À la source « ${source} », sous un schéma de son choix.`"
  >
    <form class="flex flex-col gap-3" @submit.prevent="attach(false)">
      <ui-field label="Origine">
        <ui-select v-model="origin" :items="ORIGINS" label="Origine" />
      </ui-field>
      <ui-field label="Schéma">
        <ui-input
          v-model="schema"
          :maxlength="LENGTHS.schema"
          placeholder="archive"
          spellcheck="false"
        />
      </ui-field>
      <ui-field v-if="origin === 'existing'" label="Fichier">
        <span class="flex min-w-0 items-center gap-2">
          <ui-input
            v-model="location"
            :maxlength="LENGTHS.location"
            placeholder="D:\data\archive.db"
            spellcheck="false"
            class="min-w-0 flex-1"
          />
          <connection-scheme-badge v-if="scheme" :scheme="scheme" />
        </span>
      </ui-field>
      <p class="text-sm text-fg-secondary">
        {{ blocker === 'reserved' ? 'Les schémas main et temp sont réservés.' : HINTS[origin] }}
      </p>
    </form>
    <template #footer>
      <div class="flex w-full justify-end gap-3">
        <ui-button label="Annuler" @click="source = null" />
        <ui-button label="Pour cette session" :disabled="blocker !== null" @click="attach(false)" />
        <ui-button
          label="Dans la connexion"
          variant="solid"
          tone="primary"
          :disabled="blocker !== null"
          @click="attach(true)"
        />
      </div>
    </template>
  </ui-dialog>
</template>
