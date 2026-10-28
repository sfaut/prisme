<!--
  Status bar, under a content (result grid...): a line of small secondary
  text, made of items, one element each in the default slot, which the bar
  separates by a middle dot. The items wrap as wholes when the line is too
  narrow. Its line keeps its full height, the descenders included (no
  `text-box` trim): the panel clips what goes past its bottom.
-->
<template>
  <div class="flex flex-wrap text-sm text-fg-secondary">
    <slot />
  </div>
</template>

<style scoped>
@reference "@/assets/main.css";

/* Separator before every item but the first, a step of room on each side. */
:slotted(:not(:first-child))::before {
  content: '·';
  padding-inline: --spacing(1);
}
</style>
