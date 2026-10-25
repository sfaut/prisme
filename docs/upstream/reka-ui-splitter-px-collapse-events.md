# Reka UI: `collapse` / `expand` not emitted by px panels

Draft of a ticket for [unovue/reka-ui](https://github.com/unovue/reka-ui/issues), following its bug
template, to post from the maintainer's account. Prisme works around it in `web/src/App.vue`: the
state of the console follows `isCollapsed` on each `resize` of its panel.

---

**Title:** `[Bug]: SplitterPanel with sizeUnit="px" never emits collapse / expand when resized by dragging`

### Environment

```bash
Development/Production OS: Windows 11 Pro 10.0.26300
Node version: v26.7.0
Package manager: npm
Reka UI version: 2.11.0
Vue version: 3.5.44
CSS framework: tailwindcss@4 (not involved: the reproduction below uses inline styles)
Client OS: Windows 11 Pro 10.0.26300
Browser: Chrome 156, display scaling 135 % and 150 %
```

### Link to minimal reproduction

(StackBlitz link to create from the component below.)

```vue
<script setup lang="ts">
import { SplitterGroup, SplitterPanel, SplitterResizeHandle } from 'reka-ui'
import { ref } from 'vue'

const events = ref<string[]>([])
const log = (line: string) => events.value.push(line)
</script>

<template>
  <div style="display: flex; gap: 16px">
    <div style="height: 542px; width: 300px">
      <SplitterGroup direction="vertical">
        <SplitterPanel :order="1" size-unit="px" :min-size="300">top</SplitterPanel>
        <SplitterResizeHandle style="height: 8px; background: #999" />
        <SplitterPanel
          v-slot="{ isCollapsed }"
          :order="2"
          size-unit="px"
          :default-size="220"
          :min-size="120"
          collapsible
          :collapsed-size="28"
          @resize="(size: number) => log(`resize ${size}`)"
          @collapse="log('collapse')"
          @expand="log('expand')"
        >
          isCollapsed: {{ isCollapsed }}
        </SplitterPanel>
      </SplitterGroup>
    </div>
    <pre>{{ events.join('\n') }}</pre>
  </div>
</template>
```

### Steps to reproduce

1. Open the reproduction (observed with a display scaling of 135 % and 150 %, where the group
   measures a fractional height, 541.99 px for a 542 px box).
2. Drag the handle down, below the `min-size` of the bottom panel: it collapses to 28 px.
3. Drag the handle up again: the panel expands.

### Describe the bug

The panel collapses and expands on screen, `isCollapsed` and `data-state` follow (fixed by #2742 /
#2743), but neither `collapse` nor `expand` is emitted. The log reads:

```
resize 220.00000000007145
expand
resize 173.9979425622853
resize 28.000000000095202
resize 197.998183551317
resize 239.9977347817206
```

The last notified size of the collapsed panel is `28.000000000095202`, while its `collapsed-size` is
`28`. `callPanelCallbacks` converts the size of a px panel from percent back to pixels
(`displaySize = size / 100 * groupSizeInPixels`), then decides both events with
`fuzzyNumbersEqual(displaySize, collapsedSize)`, at 10 decimals (`PRECISION`). The round trip
px → % → px, with the layout normalized to 100 %, leaves an error of about 1e-10 px, larger than that
precision: the sizes never compare equal, so `onCollapse` never runs, and since the last notified size
is never equal to `collapsedSize`, `onExpand` does not run either.

`isPanelCollapsed` had the same drift and was fixed in #2743 by a fuzzy comparison in percent
(`fuzzyCompareNumbers(panelSize, collapsedSize) <= 0`); `callPanelCallbacks` still compares in pixels,
with the strict precision.

### Expected behavior

Dragging a px panel below its `min-size` emits `collapse`; dragging it out of its collapsed size emits
`expand`, as for a panel sized in percent.

A possible fix: in `callPanelCallbacks`, decide the collapsed state in percent, as `isPanelCollapsed`
does: compare the layout size with `collapsedSize` converted to percent (the constraints from
`getPanelConstraintsInPercent`), and keep `displaySize` only for the value passed to `onResize`.

### Context & Screenshots

Workaround: listen to `resize` and read the panel's exposed `isCollapsed`:

```vue
<SplitterPanel ref="panel" ... @resize="open = !panel?.isCollapsed" />
```
