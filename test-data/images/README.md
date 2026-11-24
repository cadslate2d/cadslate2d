# Test pictures

Small pictures for the tests that need real image bytes (ADR-0042): a definition reading its header, its components, and a PNG's compressed samples (`cad-document`), and a plot embedding them (`cad-plot`). Each is 16 × 12 pixels in four quarters — red above blue on the left, green above yellow on the right — so a test or a reader can tell which way up it is.

| File | Made by | What it is |
| --- | --- | --- |
| `quarters.png` | Pillow 11.2.1, `Image.new("RGB", (16, 12))`, saved with `optimize=True` | 8-bit RGB, not interlaced |
| `quarters-grey.png` | The same, `convert("L")` | 8-bit grey |
| `quarters.jpg` | The same, `quality=95, subsampling=0` | Baseline JPEG, three components |
| `quarters-grey.jpg` | The same, `convert("L")`, `quality=95` | Baseline JPEG, one component |

The quarters are (220, 30, 30), (30, 180, 40), (30, 50, 220), and (240, 220, 30). The files are kept, not made at test time: a test reads them with `include_bytes!`.
