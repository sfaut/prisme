# Reka UI: a group with a panel in pixels, mounted hidden, gets no layout

Draft of a ticket for [unovue/reka-ui](https://github.com/unovue/reka-ui/issues), following its bug
template, to post from the maintainer's account. Prisme works around it in
`web/src/components/PivotZones.vue`: the splitters of the sections of the Pivoteur, mounted under a
folded Pivoteur (`v-show`), start again (`key`) as soon as they get a width.

Seen in Prisme (2026-11-05): after a reload with the Pivoteur folded, then unfolded, the section of
the columns took 1,004.68 px of the panel and the zones 3.60 px, instead of 174 and 204 px. In an
editor in the background at the reload, the splitter of the editor and its results
(`QueryCell.vue`, not worked around) lost its saved layout, and folding or unfolding its panels
threw `Panel size not found for panel "cell-25-editor-panel"` and `Assertion failed!`.

Measured on the reproduction below, without posting it.

---

**Title:** `[Bug]: SplitterGroup with a sizeUnit="px" panel mounted inside display: none gets no layout once shown`

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
import { ref } from 'vue'

const shown = ref(new URLSearchParams(location.search).get('hidden') !== '1')
</script>

<template>
  <button id="show" @click="shown = true">Show</button>
  <div v-show="shown" style="width: 600px; height: 100px">
    <SplitterGroup direction="horizontal">
      <SplitterPanel id="left" :order="1" size-unit="px" :default-size="150" :min-size="100">
        left
      </SplitterPanel>
      <SplitterResizeHandle style="width: 8px; background: #999" />
      <SplitterPanel id="right" :order="2">right</SplitterPanel>
    </SplitterGroup>
  </div>
</template>
```

### Steps to reproduce

1. Open the reproduction, read the width of the left panel.
2. Open it with `?hidden=1` in the address, click "Show", read the width of the left panel.
3. Drag the handle to the left.

### Describe the bug

| Group | Left panel | Right panel |
|---|---|---|
| Mounted shown | 148 px, `flex: 25 1 0px` | 444 px, `flex: 75 1 0px` |
| Mounted hidden, then shown | 588.08 px, `flex: 150 1 0px` | 3.92 px, `flex: 1 1 0px` |

Mounted hidden, then shown, the panels keep these sizes, and a drag of the handle throws at each move
of the pointer, the panels unchanged:

```
Error: Assertion failed!
    at assert
    at adjustLayoutByDelta
    at RefImpl.resizeHandler
    at setResizeHandlerState
    at updateResizeHandlerStates
    at HTMLBodyElement.handlePointerMove
```

### Expected behavior

Once shown, the group lays its panels out as a group mounted shown does: the left panel gets its
`default-size` of 150 px, and the handle resizes it.

### Context & Screenshots

Workaround: mount the group again (`key`) once its element gets a width, followed by a
`ResizeObserver`.
