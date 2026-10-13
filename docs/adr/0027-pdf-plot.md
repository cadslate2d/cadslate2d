# ADR-0027: A PDF plot of model space is the renderer's walk, written as a page

**Status:** Accepted (2026-10-13)  
**Date:** 2026-10-13  
**Kind:** Design (#187; extended for a layout, #194)  
**Design refs:** §24, §36; ADR-0001, ADR-0002, ADR-0009, ADR-0016, ADR-0020, ADR-0021

## Context

There was no way to put a drawing on paper. A vector PDF of model space at a scale is the first form of plotting, and what most readers of a 2D drawing want. Rust owns the drawing and knows no browser (ADR-0002); the browser knows how to offer a download. The committed scene already turns every entity kind into segments, arcs, and triangles (`PackedFrame::push_entity`), with its colour, lineweight, and linetype resolved against the tables.

## Decision

1. **One walk over the entity kinds, two sinks.** The frame's `push_entity` becomes `cad_render::walk_entity`, a function over a `GeometrySink` — segments with their path distance, arcs with their direction, triangles — that the frame implements for the GPU and the plot implements for a page. What prints is what is drawn: an ellipse's chords, a polyline's arc segments as arcs, a text's strokes, a dimension's layout, a hatch's pattern lines and boundary. The sink has one override: a solid hatch is its loops, filled by the even-odd rule, rather than its triangles, so a page has no seams between them.
2. **A crate of its own, `cad-plot`**, depending on `cad-render` and below (ADR-0001 amended). It lays the page out and writes PDF 1.4: one page, one uncompressed content stream, ASCII throughout, a cross-reference table a reader checks. No fonts are embedded and no text operators are used: text is its strokes, as on screen (ADR-0016), so every reader shows the same glyphs and none can search them. No images, no transparency, no layers or layouts: model space only.
3. **The page.** A paper size by name (A4 to A0, Letter, Tabloid), held landscape when the plotted area is wider than tall; a 10 mm margin; the area — the extents, or a window, clipped at its edge — centred on the page. The scale is a fit within the margins or `1:N`: a drawing unit is its unit in millimetres (a unitless drawing is taken in millimetres) divided by N, in points. Arcs are cubic Béziers of at most a quarter turn each, within 0.03% of the radius.
4. **Looks on paper.** A colour is the index palette's RGB; colour 7, the foreground, is black on paper, and monochrome plots everything black. A lineweight is a width on paper, never scaled: hundredths of a millimetre in points, the default and "lineweights off" at 0.25 mm. A linetype is a dash array in page units, with LTSCALE, continuing through a polyline's vertices because consecutive pieces that meet are one subpath. Text, dimensions, and hatches are continuous, as on screen.
5. **A layout's page is its sheet (#194).** `PLOT` on a layout plots the space the commands act in (`CommandContext::space`; worked through a viewport, that is the model). The page is the layout's sheet — the paper size its plot settings carry (groups 44 and 45), or A3 landscape for a layout made here — held as the sheet is, with no margin of its own: the sheet's margins are its printable area. The paper prompt offers the sheet by its paper's name, and a paper named becomes the sheet, kept on the layout's row (`Layout::paper_size`, `set_layout_sheet`) as one undoable change the writer puts back into the plot settings; the session's own paper default is not touched. The area is the sheet or a window of it; a fit of the sheet is one to one, and `1:N` is a paper millimetre over N. The paper's entities are placed by the page; a viewport is a window, not a thing on the paper — its frame is not plotted — and for each viewport that is on, in ID order, the model's entities are drawn inside a nested graphics state clipped to the frame, through a `Placement` composed of the viewport's scale and the page's, with the look set afresh inside it. Lineweights stay a width on paper; a linetype's dashes scale with the viewport. A viewport that is off, or on a layer that is off, draws nothing. The file is named after the drawing and the layout (`plan-Layout1.pdf`).
6. **The host does the download.** `PLOT` (`PDFOUT`) asks for the paper, the area, the scale, the lineweights, and the colour, keeps them as the next defaults, and raises `HostRequest::Plot`; the host pulls the PDF in one call (`plot_pdf`, in the boundary snapshot) and saves it under the drawing's name with a `.pdf` extension, through the same file access as a DXF.

## Evidence

The plot of one of everything (`the_sample_plot_for_a_reader`, written to `target/plot-sample.pdf`) was opened by two readers on 2026-10-13: poppler (`pdfinfo` reports one A4 landscape page, `pdftotext` reads it without error) and Ghostscript (`gs` rasterised it; the line, circle, arc, ellipse, spline, bulged polyline, text, paragraph, turned block, dimension with its text, solid hatch, and patterned hatch are all where they should be). A person should still look at it in a desktop viewer before the acceptance box is final.

## Alternatives considered

- **A canvas-to-PDF library in the browser.** Would raster the drawing, or need a second walk over the entity kinds in TypeScript; the geometry is Rust's.
- **Fonts embedded, text as text.** Searchable, but a different face from the screen's and a font subsetting problem of its own; the stroke font is the drawing's text (ADR-0016).
- **Compressed streams.** Smaller files, but a binary file and a deflate implementation; text as strokes is the bulk, and a page of it is tens of kilobytes.

## Consequences

- `PLOT` plots every entity kind (`every_entity_kind_lands_on_the_page`); lineweights measure on the page at their width (`lineweights_measure_on_the_page_at_their_width`); every primitive of a random drawing lands inside the printable area (`every_primitive_lands_inside_the_page`).
- A layout with two viewports at different scales plots both at their scale, clipped, with a title line at one to one and no frames (`a_layout_plots_its_paper_at_one_to_one_and_the_model_through_each_viewport_at_its_scale`); on a random layout every point the paper draws is on the sheet, every viewport's clip is its frame on the page, and every point drawn through a viewport is a model point at the viewport's scale (`every_primitive_of_a_layout_lands_on_the_sheet_and_the_model_through_each_viewport`). The layouts fixture's `Layout1` plots its viewport and its title line end to end (`paper-space.spec.ts`).
- The frame's walk is shared, so a new entity kind is added once (in `walk_entity`) for the screen and the page alike.
- Plotting is not saving: the drawing stays unexported, and no autosave is written for it.

## Revisit when

- A layout's other plot settings — its margins, its scale, its plot area — are wanted on the page as the file carries them; today only the paper size is read, and the sheet's margins shape `MVIEW Fit` alone.
- A plot style table (colour-dependent lineweights) is wanted, or searchable text.
