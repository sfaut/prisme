<!--
  Settings of a MySQL / MariaDB connection, on a single page, in lists of
  settings by subject: Général (name), Accès (host, port, account, password,
  database), Session (queries run on connection, keep-alive delay),
  Navigateur (system databases). Each setting says what it changes under its
  name. Shown by the connections manager for a saved connection and for the
  settings step of a creation.

  The interface does not know the saved password: the field starts empty,
  and its description says what an empty field means (`password`): kept for
  a saved connection, that of the copied connection for a copy.

  "À la connexion" receives queries run when each connection to the database
  opens, separated by ";": only session settings (`SET NAMES`,
  `SET SESSION sql_mode`...) are allowed, the server refuses the rest on
  saving.

  The maximum lengths repeat the server's (`connectionForm.ts`); the other
  checks are made by the server, whose message the manager shows.
-->
<script setup lang="ts">
import { Eye, EyeOff } from '@lucide/vue'
import { computed, ref, useId } from 'vue'

import { type ConnectionForm, LENGTHS, SESSION_TIMEOUT_MAX } from './connectionForm'

import UiButton from '@/components/ui/UiButton.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiSettingList from '@/components/ui/UiSettingList.vue'
import UiSettingRow from '@/components/ui/UiSettingRow.vue'
import UiSwitch from '@/components/ui/UiSwitch.vue'
import UiTextarea from '@/components/ui/UiTextarea.vue'

const props = defineProps<{
  /**
   * Meaning of an empty password: none yet (`new`), the saved one (`saved`),
   * that of the copied connection (`copy`).
   */
  password: 'new' | 'saved' | 'copy'
}>()

/**
 * Input, changed in place.
 */
const form = defineModel<ConnectionForm>('form', { required: true })

/**
 * Prefix of the ids of the fields, that their names label.
 */
const id = useId()

/**
 * Is the password shown in clear?
 */
const showPassword = ref(false)

/**
 * Description of the password, by the meaning of an empty field.
 */
const passwordDescription = computed(
  () =>
    ({
      new: 'Chiffré dans la base de Prisme, jamais renvoyé au navigateur.',
      saved: 'Laissé vide, il reste inchangé.',
      copy: 'Laissé vide, il reprend celui de la connexion copiée.',
    })[props.password],
)
</script>

<template>
  <ui-setting-list title="Général">
    <ui-setting-row
      label="Nom"
      description="Nom de la source dans le navigateur et les éditeurs."
      :for="`${id}-name`"
    >
      <ui-input
        :id="`${id}-name`"
        v-model="form.name"
        :maxlength="LENGTHS.name"
        placeholder="prod, local…"
        class="w-60"
      />
    </ui-setting-row>
  </ui-setting-list>

  <ui-setting-list title="Accès">
    <ui-setting-row label="Hôte" description="Nom ou adresse du serveur." :for="`${id}-host`">
      <ui-input :id="`${id}-host`" v-model="form.host" :maxlength="LENGTHS.host" class="w-60" />
    </ui-setting-row>
    <ui-setting-row label="Port" description="De 1 à 65 535." :for="`${id}-port`">
      <ui-input
        :id="`${id}-port`"
        v-model="form.port"
        type="number"
        :min="1"
        :max="65535"
        required
        class="w-27"
      />
    </ui-setting-row>
    <ui-setting-row
      label="Compte"
      description="Un compte en lecture seule protège des écritures."
      :for="`${id}-username`"
    >
      <ui-input
        :id="`${id}-username`"
        v-model="form.username"
        :maxlength="LENGTHS.username"
        class="w-60"
      />
    </ui-setting-row>
    <ui-setting-row label="Mot de passe" :description="passwordDescription" :for="`${id}-password`">
      <ui-input
        :id="`${id}-password`"
        v-model="form.password"
        :type="showPassword ? 'text' : 'password'"
        autocomplete="new-password"
        :placeholder="password === 'new' ? '' : 'inchangé'"
        class="w-60"
      >
        <template #trailing>
          <ui-button
            :icon="showPassword ? EyeOff : Eye"
            variant="ghost"
            small
            :aria-label="showPassword ? 'Masquer le mot de passe' : 'Afficher le mot de passe'"
            @click="showPassword = !showPassword"
          />
        </template>
      </ui-input>
    </ui-setting-row>
    <ui-setting-row label="Base" description="Base ouverte par défaut." :for="`${id}-database`">
      <ui-input
        :id="`${id}-database`"
        v-model="form.database_name"
        :maxlength="LENGTHS.databaseName"
        placeholder="facultatif"
        class="w-60"
      />
    </ui-setting-row>
  </ui-setting-list>

  <ui-setting-list title="Session">
    <ui-setting-row
      label="À la connexion"
      description="Réglages de la session exécutés à l’ouverture de chaque connexion, séparés par des « ; »."
      :for="`${id}-init`"
      stacked
    >
      <ui-textarea
        :id="`${id}-init`"
        v-model="form.init_sql"
        :maxlength="LENGTHS.initSql"
        rows="3"
        placeholder="SET NAMES utf8mb4;&#10;SET SESSION sql_mode = 'ANSI_QUOTES';"
        class="w-full"
      />
    </ui-setting-row>
    <ui-setting-row
      label="Session gardée"
      description="La session (connexion, transactions, requêtes en cours) reste ouverte ce temps après la fermeture de la page, qui s’y raccroche à son retour ; 0 la ferme aussitôt."
      :for="`${id}-timeout`"
    >
      <ui-input
        :id="`${id}-timeout`"
        v-model="form.session_timeout_minutes"
        type="number"
        :min="0"
        :max="SESSION_TIMEOUT_MAX"
        required
        class="w-27"
      />
      <span class="text-fg-secondary">min</span>
    </ui-setting-row>
  </ui-setting-list>

  <ui-setting-list title="Navigateur">
    <ui-setting-row
      label="Afficher les bases système"
      description="information_schema, mysql, performance_schema, sys."
      :for="`${id}-system`"
    >
      <ui-switch :id="`${id}-system`" v-model="form.show_system_schemas" />
    </ui-setting-row>
  </ui-setting-list>
</template>
