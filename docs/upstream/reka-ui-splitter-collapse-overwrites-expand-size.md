# Reka UI: `collapse()` on a collapsed panel overwrites its expand size

Draft of a ticket for [unovue/reka-ui](https://github.com/unovue/reka-ui/issues), following its bug
template, to post from the maintainer's account. Prisme works around it in `web/src/App.vue`
(`showPanel`): a panel is only collapsed when it is expanded, and expanded when it is collapsed.

---

**Title:** `[Bug]: SplitterPanel collapse() on an already collapsed panel replaces its expand size, expand() then opens at minSize`

### Environment

```bash
Development/Production OS: Windows 11 Pro 10.0.26300
Node version: v26.7.0
Package manager: npm
Reka UI version: 2.11.0
Vue version: 3.5.44
CSS framework: none (the reproduction below uses inline styles)
Client OS: Windows 11 Pro 10.0.26300
Browser: Chrome 156
```

### Link to minimal reproduction

(StackBlitz link to create from the component below.)

```vue
<script setup lang="ts">
import { SplitterGroup, SplitterPanel, SplitterResizeHandle } from 'reka-ui'
import { ref, useTemplateRef } from 'vue'

const panel = useTemplateRef<InstanceType<typeof SplitterPanel>>('panel')
const saved = ref('')

function run(action: 'collapse' | 'expand') {
  panel.value?.[action]()
  setTimeout(() => {
    const state = JSON.parse(localStorage.getItem('reka:repro') ?? '{}')
    saved.value = JSON.stringify(Object.values<any>(state)[0]?.expandToSizes ?? {})
  }, 300)
}
</script>

<template>
  <button @click="run('collapse')">collapse()</button>
  <button @click="run('expand')">expand()</button>
  <div style="height: 120px; width: 601.5px; border: 1px solid #999">
    <SplitterGroup direction="horizontal" auto-save-id="repro">
      <SplitterPanel :order="1" size-unit="px" :min-size="200">left</SplitterPanel>
      <SplitterResizeHandle style="width: 8px; background: #999" />
      <SplitterPanel
        ref="panel"
        :order="2"
        size-unit="px"
        :default-size="300"
        :min-size="200"
        collapsible
        :collapsed-size="28"
      >
        right
      </SplitterPanel>
    </SplitterGroup>
  </div>
  <pre>expandToSizes: {{ saved }}</pre>
</template>
```

### Steps to reproduce

1. Open the reproduction with an empty `localStorage`: the right panel is 300 px wide.
2. Click `collapse()`: the panel collapses to 28 px; the saved `expandToSizes` holds 300.
3. Reload the page: the layout comes back from `autoSaveId`, the panel collapsed.
4. Click `collapse()` again, on the panel already collapsed.
5. Click `expand()`.

### Describe the bug

At step 4, the panel stays collapsed, but its expand size is replaced by its collapsed size; at step 5,
it expands to its `min-size` (200 px) instead of 300 px. The log of the reproduction:

```
loaded:            296.15625 px, expandToSizes: {}
collapse():        27.666667938232422 px, expandToSizes: {"reka-splitter-panel-v-3":300.0000000000105}
reloaded:          27.666667938232422 px, expandToSizes: {"reka-splitter-panel-v-3":300.0000000000105}
collapse() again:  27.666667938232422 px, expandToSizes: {"reka-splitter-panel-v-3":27.999999999808498}
expand():          197.6354217529297 px, expandToSizes: {"reka-splitter-panel-v-3":27.999999999808498}
```

`collapsePanel` (`SplitterGroup.vue`) only skips a panel already collapsed when its size is strictly
equal to its collapsed size: `if (panelSize !== collapsedSize)`. Both are percentages; for a px panel,
the layout restored from the storage and the collapsed size converted to percent differ by about
1e-10, so the panel is taken as expanded: its current size, the collapsed one, is stored as the size
to expand to. `expandPanel` then finds a size below `minSize` and falls back to `minSize`.

`isPanelCollapsed` and `expandPanel` already compare these sizes with `fuzzyCompareNumbers(panelSize,
collapsedSize) <= 0`; `collapsePanel` is the only one left with a strict comparison. An application
that sets the collapsed state of its panels from its own state on mount (calling `collapse()` for a
panel that should be collapsed) hits it on every reload.

### Expected behavior

`collapse()` on a panel already collapsed does nothing, and `expand()` restores the size the panel had
before it was collapsed (300 px here).

A possible fix: in `collapsePanel`, test `fuzzyCompareNumbers(panelSize, collapsedSize) > 0`, as
`isPanelCollapsed` and `expandPanel` do.

### Context & Screenshots

Workaround: only call `collapse()` on an expanded panel:

```ts
if (!panel.isCollapsed) {
  panel.collapse()
}
```
