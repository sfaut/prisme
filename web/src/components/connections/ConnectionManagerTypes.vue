<!--
  Choice of the kind of source of a new connection, first step of a creation
  in the connections manager: one card per kind (`sourceTypes.ts`), two per
  row, grouped by family, all of the same height. The chosen card is framed
  in the primary color. The kinds not supported yet show greyed out, marked
  "à venir", and cannot be chosen: they say where Prisme is going.
-->
<script setup lang="ts">
import { SOURCE_FAMILIES, type SourceTypeKey } from './sourceTypes'

/**
 * Kind chosen.
 */
const type = defineModel<SourceTypeKey>('type', { required: true })
</script>

<template>
  <section
    v-for="family in SOURCE_FAMILIES"
    :key="family.key"
    class="flex flex-col gap-2"
    :aria-label="family.heading"
  >
    <h4 class="text-sm font-semibold text-fg-secondary">{{ family.heading }}</h4>
    <div class="grid grid-cols-2 gap-3">
      <button
        v-for="item in family.types"
        :key="item.key"
        type="button"
        class="flex h-26 cursor-pointer items-center gap-4 rounded-xl border p-4 text-start disabled:cursor-not-allowed disabled:opacity-50"
        :class="
          type === item.key
            ? 'border-primary bg-primary/10'
            : 'border-line bg-surface enabled:hover:bg-surface-raised'
        "
        :disabled="!item.ready"
        :aria-pressed="type === item.key"
        @click="type = item.key"
      >
        <span
          class="grid size-12 shrink-0 place-items-center rounded-lg bg-primary/15 text-primary"
          aria-hidden="true"
        >
          <component :is="item.icon" class="size-5" />
        </span>
        <span class="flex min-w-0 flex-col gap-1">
          <span class="flex items-center gap-2 font-semibold text-fg-strong">
            {{ item.label }}
            <span
              v-if="!item.ready"
              class="rounded-full bg-fg-secondary/15 px-1 text-xs font-medium text-fg-secondary"
            >
              <span class="text-trim">à venir</span>
            </span>
          </span>
          <span class="text-sm text-fg-secondary">{{ item.description }}</span>
        </span>
      </button>
    </div>
  </section>
</template>
