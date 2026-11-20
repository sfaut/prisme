<!--
  Header of a panel of the layout (object navigator, data explorer, editor,
  Pivoteur, console): its title, or what replaces it (default slot), what
  follows the title on its line (`inline` slot: the buttons of the editor,
  the tabs of the console), a line under the title (`subtitle` slot) and the
  panel's actions (`actions` slot), at the right end. The panel opens and
  closes from the activity bar of the editor's tab (`PanelActivityBar.vue`).
  The title, what follows it and the actions share a first line, at least of
  a control's height, centred on it, whatever comes below (subtitle).
-->
<script setup lang="ts">
defineProps<{
  /**
   * Title of the panel.
   */
  title: string
}>()
</script>

<template>
  <div class="flex flex-col gap-1">
    <div class="flex min-h-8 min-w-0 items-center justify-between gap-3">
      <div class="flex min-w-0 flex-1 items-center gap-3 self-stretch">
        <slot>
          <h2 class="shrink-0 text-base font-semibold text-fg-strong">{{ title }}</h2>
        </slot>
        <slot name="inline" />
      </div>
      <div class="flex items-center self-stretch">
        <slot name="actions" />
      </div>
    </div>
    <slot name="subtitle" />
  </div>
</template>
