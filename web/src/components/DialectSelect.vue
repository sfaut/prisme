<!--
  List of the dialects the user can write in: the native SQL of the data
  source, then each version of MySQL and MariaDB offered by the server. With
  `inherited`, a first choice takes the preferred dialect of the profile
  (empty string in the model), for an editor that does not choose its own.
-->
<script setup lang="ts">
import { computed, onMounted } from 'vue'

import { useDialectsStore } from '@/stores/dialects'

import { choiceItems, choiceLabel } from './dialectChoice'
import UiSelect from './ui/UiSelect.vue'
import { notify } from './ui/toasts'
import { errorMessage } from '@/api/errorMessage'

const props = defineProps<{
  /**
   * Preferred dialect of the profile, offered as the first choice; absent:
   * no such choice.
   */
  inherited?: string
  /**
   * Name of the list, read by screen readers.
   */
  label: string
}>()

/**
 * Chosen dialect, `source` or a dialect with its version; empty string for
 * the preferred dialect of the profile.
 */
const model = defineModel<string>({ required: true })

/**
 * Value of the choice "dialect of the profile" in the list, which refuses an
 * empty value.
 */
const INHERITED = 'profile'

const dialects = useDialectsStore()

/**
 * Choices of the list.
 */
const items = computed(() => [
  ...(props.inherited === undefined
    ? []
    : [{ label: `Profil : ${choiceLabel(props.inherited)}`, value: INHERITED }]),
  ...choiceItems(dialects.options),
])

/**
 * Value shown by the list.
 */
const selected = computed({
  get: () => (model.value === '' ? INHERITED : model.value),
  set: (value: string | undefined) => {
    if (value !== undefined) {
      model.value = value === INHERITED ? '' : value
    }
  },
})

onMounted(() => {
  dialects.load().catch((failure: unknown) => {
    notify(`Dialectes illisibles : ${errorMessage(failure)}`, 'error')
  })
})
</script>

<template>
  <ui-select v-model="selected" :items="items" :label="label" />
</template>
