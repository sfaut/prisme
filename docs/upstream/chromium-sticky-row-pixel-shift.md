# Chromium: a sticky row draws its lines one pixel off the rows below

Draft of a ticket for [Chromium](https://issues.chromium.org/issues/new) (component to choose
when posting: Blink>Compositing or Blink>Paint), to post from the maintainer's account. Prisme has
no workaround: the limit is documented in `docs/DEVELOPER.md`, section "Lines". The closest ticket,
[401693546](https://issues.chromium.org/issues/401693546) ("position: sticky elements are shifted
1px to the left"), was fixed in M135 by
[Avoid rounding inconsistency between Blink and cc](https://chromium-review.googlesource.com/c/chromium/src/+/6340249),
a heuristic for values starting at -0.5; the case below remains in Chrome 156.

Measured on 2026-11-04 with the page below, in headless Chrome started with
`--force-device-scale-factor`: for each left offset of the scroll container, a screenshot of the
sticky row and of the row below it, the dark pixels read across each row.

---

**Title:** `position: sticky row inside a scroll container: its borders are drawn 1 device pixel off those of the non-sticky rows below, at most fractional offsets`

### Chrome version

Chrome 156.0.8078.47, Windows 11 Pro 10.0.26300. Measured in headless Chrome started with
`--force-device-scale-factor` set to 1, 1.25, 1.5, 1.75 and 2.

### Steps to reproduce

1. Open the page below at a display scale of 150 % (or start Chrome with
   `--force-device-scale-factor=1.5`).
2. Move the slider: it sets the left offset of the scroll container, from 20px to 22px by steps
   of 0.05px.
3. At each offset, take a screenshot and zoom into it: compare the vertical lines of the grey
   sticky row with those of the white rows below.

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Sticky row: vertical lines one pixel off the rows below</title>
    <style>
      body {
        margin: 0;
        font: 14px sans-serif;
      }

      p {
        margin: 8px;
      }

      /* The left offset of the scroll container, set by the slider. */
      .scroller {
        position: absolute;
        top: 80px;
        left: var(--left, 20px);
        width: 600px;
        height: 200px;
        overflow: auto;
      }

      .row {
        display: grid;
        grid-template-columns: 50.3px 70.7px 90.45px 60.2px 80.9px 300px;
        width: max-content;
        height: 24px;
      }

      .row > div {
        border-left: 1px solid black;
      }

      .sticky {
        position: sticky;
        top: 0;
        background: #ddd;
      }
    </style>
  </head>
  <body>
    <p>
      Display scale 150 % (or <code>--force-device-scale-factor=1.5</code>). Left offset of the
      scroll container: <input id="left" type="range" min="20" max="22" step="0.05" value="20" />
      <output id="value">20px</output>. At most offsets, the vertical lines of the grey sticky row
      stand one device pixel beside those of the rows below, though their layout positions are
      equal.
    </p>
    <div class="scroller"></div>
    <script>
      const scroller = document.querySelector('.scroller')
      for (let row = 0; row < 12; row++) {
        const line = scroller.appendChild(document.createElement('div'))
        line.className = row === 0 ? 'row sticky' : 'row'
        for (let cell = 0; cell < 6; cell++) line.append(document.createElement('div'))
      }
      const left = document.getElementById('left')
      const value = document.getElementById('value')
      left.addEventListener('input', () => {
        document.body.style.setProperty('--left', `${left.value}px`)
        value.textContent = `${left.value}px`
      })
    </script>
  </body>
</html>
```

### Expected result

The cells of the sticky row and of the rows below have the same layout positions
(`getBoundingClientRect().left` is equal for each column, at every offset): their left borders are
drawn on the same device pixels, as they are when the row is not sticky.

### Actual result

At most fractional offsets, some borders of the sticky row are drawn one device pixel beside the
same borders of the rows below. The sticky row moves as a whole, by one device pixel at a time,
while each border below follows its own rounding. For example, at a scale of 150 % and a left
offset of 20.05px, the borders are drawn at these device pixels:

| Row | Columns 1 to 6 |
| --- | --- |
| sticky | 30, 105, 211, 347, 437, 559 |
| rows below | 30, 106, 212, 347, 438, 559 |

Over the 41 offsets of the slider (20px to 22px by 0.05px):

| Device scale factor | Offsets with borders apart |
| --- | --- |
| 1 | 36 of 41 |
| 1.25 | 30 of 41 |
| 1.5 | 28 of 41 |
| 1.75 | 26 of 41 |
| 2 | 28 of 41 |

With `position: relative` in place of `position: sticky` on the grey row, the borders match at
all 41 offsets. In the application where it was found, drawing the lines of the sticky row with
positioned pseudo-elements, giving them their own stacking context (`z-index`), or removing the
`transform` that places the rows below did not change the result.

### Impact

Data grids with a sticky header row (spreadsheet-like tables, SQL clients) show a one-pixel jog
between the column lines of the header and those of the body, and anything centered on a column
line in the header (resize handles, drop markers) is off center on that line, according to the
horizontal position of the grid in the page.
