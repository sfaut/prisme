# Reka UI: a panel sized in pixels does not get its size

Draft of a ticket for [unovue/reka-ui](https://github.com/unovue/reka-ui/issues), following its bug
template, to post from the maintainer's account. Prisme works around it in
`web/src/components/PanelRail.vue`: the panel that holds the rail of a folded panel takes exactly the
size of the rail (`flex: 0 0 var(--rail-size) !important`).

Seen in Prisme (2026-11-05): the folded panels of 24 px, among handles drawn as lines of 1 screen
pixel, measured 23.90 to 24.02 px; at a display scaling of 125 %, the rail of the Pivoteur showed
29 screen pixels where that of the console showed 30.

Measured on the reproduction below, without posting it: with a handle of 0 px, the panel measures
24 px, except for a group of 1,197 px (24.0625 px, `flex: 2.01`); with a handle of 1 px, 23.94 to
24.03 px. Reading of the code, kept out of the ticket: the size in pixels becomes a share of the
whole group, handles included (24 / 400 gives `6`), while `flex-grow` shares only the room the
handles leave (6 % of 392 px is 23.52 px); `computePanelFlexBoxStyle` writes the share with
`toPrecision(3)`.

---

**Title:** `[Bug]: SplitterPanel with sizeUnit="px" does not get its size in pixels`

### Environment

```bash
Development/Production OS: Windows 11 Pro 10.0.26300
Node version: v26.7.0
Package manager: npm
Reka UI version: 2.11.0
Vue version: 3.5.44
CSS framework: none (the reproduction below uses inline styles)
Client OS: Windows 11 Pro 10.0.26300
Browser: Chrome 156, display scaling 100 %
```

### Link to minimal reproduction

(StackBlitz link to create from the component below.)

```vue
<script setup lang="ts">
import { SplitterGroup, SplitterPanel, SplitterResizeHandle } from 'reka-ui'
import { onMounted, ref } from 'vue'

const groupHeight = Number(new URLSearchParams(location.search).get('height') ?? '813')
const panel = ref<{ $el: HTMLElement } | null>(null)
const measured = ref('')

onMounted(() => {
  const element = panel.value!.$el
  new ResizeObserver(() => {
    measured.value = `height ${element.getBoundingClientRect().height} px, style "${element.getAttribute('style')}"`
  }).observe(element)
})
</script>

<template>
  <div :style="{ height: `${groupHeight}px`, width: '300px' }">
    <SplitterGroup direction="vertical">
      <SplitterPanel :order="1">top</SplitterPanel>
      <SplitterResizeHandle style="height: 8px; background: #999" />
      <SplitterPanel ref="panel" :order="2" size-unit="px" :default-size="24" :min-size="24">
        bottom
      </SplitterPanel>
    </SplitterGroup>
  </div>
  <pre>{{ measured }}</pre>
</template>
```

### Steps to reproduce

1. Open the reproduction, with `?height=400` in the address, then 600, 800, 813, 1000 and 1197.
2. Read the height of the bottom panel, `default-size="24"` with `size-unit="px"`.

### Describe the bug

The panel never measures 24 px:

| Height of the group | Height of the panel | Style of the panel |
|---|---|---|
| 400 px | 23.515625 px | `flex: 6 1 0px` |
| 600 px | 23.6875 px | `flex: 4 1 0px` |
| 800 px | 23.765625 px | `flex: 3 1 0px` |
| 813 px | 23.765625 px | `flex: 2.95 1 0px` |
| 1000 px | 23.8125 px | `flex: 2.4 1 0px` |
| 1197 px | 23.890625 px | `flex: 2.01 1 0px` |

### Expected behavior

A panel sized in pixels measures its size, 24 px here, whatever the height of its group and of its
handles, as the documentation says: "Use `sizeUnit="px"` when you need a panel to stay a fixed pixel
width".

### Context & Screenshots

Workaround: give the panel its size in CSS, over the inline style:

```css
[data-panel].fixed-24 {
  flex: 0 0 24px !important;
}
```
