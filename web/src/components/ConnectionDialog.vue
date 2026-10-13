<!--
  Fenêtre de création ou de modification d'une connexion MySQL / MariaDB.

  En modification, la saisie part de la connexion enregistrée, sauf le mot de
  passe, que l'interface ne connaît pas : laissé vide, il reste inchangé.
  « Tester » se connecte avec la saisie en cours, sans l'enregistrer, et
  affiche la variante et la version du serveur.

  Les longueurs maximales reprennent celles du serveur
  (`crates/server/src/connections.rs`). Les autres contrôles sont faits par le
  serveur, dont le message s'affiche dans la fenêtre en cas de refus.

  Composants de Nuxt UI (`UModal`, `UFormField`, `UInput`...), importés
  automatiquement par son plugin Vite.
-->
<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'

import { probeSettings, serverLabel } from '@/api/connections'
import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import type { NewConnection } from '@/api/generated/NewConnection'
import { useConnectionsStore } from '@/stores/connections'

/**
 * Fenêtre ouverte ou fermée.
 */
const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  /**
   * Connexion à modifier, `null` pour en créer une.
   */
  connection: ConnectionSummary | null
}>()

const emit = defineEmits<{
  /**
   * La connexion modifiée est enregistrée.
   */
  updated: [connectionId: number]
}>()

const store = useConnectionsStore()

/**
 * Saisie en cours.
 */
const form = reactive<Required<NewConnection>>(emptyForm())

/**
 * Message du serveur si l'enregistrement a échoué.
 */
const error = ref<string | null>(null)

/**
 * Enregistrement en cours.
 */
const saving = ref(false)

/**
 * Test de la saisie en cours.
 */
const probing = ref(false)

/**
 * Mot de passe affiché en clair ?
 */
const showPassword = ref(false)

/**
 * Résultat du dernier test réussi (variante et version du serveur), `null` si aucun.
 */
const probed = ref<string | null>(null)

/**
 * Titre de la fenêtre.
 */
const title = computed(() =>
  props.connection === null ? 'Nouvelle connexion' : 'Modifier la connexion',
)

/**
 * Saisie vide, avec le port par défaut de MySQL.
 */
function emptyForm(): Required<NewConnection> {
  return { name: '', host: '127.0.0.1', port: 3306, username: '', password: '', database_name: '' }
}

/**
 * Saisie de départ : la connexion à modifier, mot de passe vide, ou une saisie vide.
 *
 * @param connection Connexion à modifier, `null` pour en créer une.
 */
function initialForm(connection: ConnectionSummary | null): Required<NewConnection> {
  if (connection === null) {
    return emptyForm()
  }
  const { name, host, port, username, database_name } = connection
  return { name, host, port, username, password: '', database_name }
}

// Chaque ouverture repart de la connexion choisie, sans l'erreur précédente.
watch(open, (isOpen) => {
  if (isOpen) {
    Object.assign(form, initialForm(props.connection))
    error.value = null
    probed.value = null
  }
})

/**
 * Se connecte avec la saisie en cours, sans l'enregistrer. En modification,
 * un mot de passe vide reprend celui de la connexion enregistrée.
 */
async function probe(): Promise<void> {
  probing.value = true
  error.value = null
  probed.value = null
  try {
    const info = await probeSettings({ connection: { ...form }, saved_id: props.connection?.id })
    probed.value = serverLabel(info)
  } catch (failure) {
    error.value = failure instanceof Error ? failure.message : String(failure)
  } finally {
    probing.value = false
  }
}

/**
 * Enregistre la connexion, puis ferme la fenêtre.
 */
async function save(): Promise<void> {
  saving.value = true
  error.value = null
  try {
    if (props.connection === null) {
      await store.create({ ...form })
    } else {
      await store.update(props.connection.id, { ...form })
      emit('updated', props.connection.id)
    }
    open.value = false
  } catch (failure) {
    error.value = failure instanceof Error ? failure.message : String(failure)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <u-modal v-model:open="open" :title="title" :ui="{ content: 'max-w-md' }">
    <template #body>
      <form id="connection-form" class="flex flex-col gap-3" @submit.prevent="save">
        <u-form-field label="Nom" orientation="horizontal" size="sm">
          <u-input
            v-model="form.name"
            :maxlength="100"
            placeholder="prod, local..."
            size="sm"
            class="w-60"
          />
        </u-form-field>
        <u-form-field label="Hôte" orientation="horizontal" size="sm">
          <u-input v-model="form.host" :maxlength="253" size="sm" class="w-60" />
        </u-form-field>
        <u-form-field label="Port" orientation="horizontal" size="sm">
          <u-input-number
            v-model="form.port"
            :min="1"
            :max="65535"
            :increment="false"
            :decrement="false"
            :format-options="{ useGrouping: false }"
            size="sm"
            class="w-60"
          />
        </u-form-field>
        <u-form-field label="Compte" orientation="horizontal" size="sm">
          <u-input v-model="form.username" :maxlength="80" size="sm" class="w-60" />
        </u-form-field>
        <u-form-field label="Mot de passe" orientation="horizontal" size="sm">
          <u-input
            v-model="form.password"
            :type="showPassword ? 'text' : 'password'"
            autocomplete="new-password"
            :placeholder="connection === null ? '' : 'inchangé'"
            size="sm"
            class="w-60"
          >
            <template #trailing>
              <u-button
                :icon="showPassword ? 'i-lucide-eye-off' : 'i-lucide-eye'"
                color="neutral"
                variant="link"
                size="xs"
                :aria-label="showPassword ? 'Masquer le mot de passe' : 'Afficher le mot de passe'"
                @click="showPassword = !showPassword"
              />
            </template>
          </u-input>
        </u-form-field>
        <u-form-field label="Base" orientation="horizontal" size="sm">
          <u-input
            v-model="form.database_name"
            :maxlength="64"
            placeholder="facultatif"
            size="sm"
            class="w-60"
          />
        </u-form-field>
        <u-alert v-if="error" color="error" variant="subtle" :title="error" />
        <u-alert v-if="probed" color="success" variant="subtle" :title="probed" />
      </form>
    </template>
    <template #footer>
      <div class="flex w-full gap-2">
        <u-button
          color="neutral"
          variant="outline"
          label="Tester"
          :loading="probing"
          @click="probe"
        />
        <u-button
          type="submit"
          form="connection-form"
          label="Enregistrer"
          :loading="saving"
          block
          class="flex-1"
        />
      </div>
    </template>
  </u-modal>
</template>
