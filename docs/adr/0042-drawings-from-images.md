# ADR-0042: An image is an underlay in the drawing, and a request may carry one

**Status:** Proposed (2026-11-24). Asked for on 2026-11-24 ("Plan #366"). The maintainer chose:
- photos, scans, and PDF pages as inputs;
- the image kept in the drawing as an underlay;
- the image never stored by the service;
- reading an image on the service's paid plan (the service's repository).

Not to be built on until accepted, and its spike (§9, #523) has measured the model. The service's half is cadslate2d/cadslate2d-ai-service#33.

**Date:** 2026-11-24  
**Issue:** #523 · **Epic:** #366, milestone M27  
**Kind:** Architecture, the render boundary (a fourth committed instance kind, amending ADR-0003 and ADR-0021), files, and the intent IR (one new operation)  
**Design refs:** §4, §12, §24, §25, §43, §45; ADR-0002, ADR-0003, ADR-0019, ADR-0021, ADR-0027, ADR-0032, ADR-0037, ADR-0038

## Context

Drafters start from paper as often as from nothing:
- a hand sketch with its sizes written on it;
- a scanned plan of a building to extend;
- a PDF a client sent.

Today they redraw it by eye, with the original in another window. #366 asks for the AI to make the editable drawing from the image. StructureClaw's benchmark (arXiv 2607.14896) counts multimodal reconstruction among the tasks a drafting agent should do.

Two things are asked at once, and they are separable:
1. **The image in the drawing,** placed and scaled under what is drawn. Classic CAD has it as an attached raster image: AutoCAD's `IMAGEATTACH`, DXF's `IMAGE` and `IMAGEDEF`. It is useful without any AI: tracing by hand, checking a drawing against its original.
2. **The image with a request.** The model sees it beside the words and answers with an intent document, as for words (ADR-0037). Its answer is reviewed and checked as any answer is (#349, #362), and previewed with the image under it.

What makes this hard here:
- **The engine has no raster at all.** The committed scene is three instance kinds — segments, arcs, triangles (ADR-0003, ADR-0009, ADR-0021). Rust never touches the GPU. The autosave is a DXF export (ADR-0019), and DXF's `IMAGE` only names an outside file.
- **Pixels are user content.** §43 says drawing content goes to the service only with the user's consent to that request and is never stored there.
- **A model reads an image approximately.** Where a sketch writes its sizes they are exact; where it does not, pixels are only a guess. A drawing guessed from pixels must not pass for one measured.

## Decision

### 1. An image is an entity with a definition, as DXF has it

An **image definition** is a row of a new table, changed in transactions like every table (§19):
- its name;
- its pixel width and height;
- its bytes, encoded as the host made them (§3);
- a digest of the bytes, which names it in files and dedupes two attaches of the same image.

An **image** entity (`Entity::Image`) refers to its definition by row and places it:
- an insertion point, the image's lower-left corner;
- `u` and `v` vectors, one pixel's width and height in drawing units, so any affine `transformed` maps it;
- a fade, 0–100 %, as AutoCAD's;
- whether its frame is drawn.

**Its geometry is its frame.** `Entity::primitives` gives the four edges, so selection, picking, snapping to its corners, bounds, MOVE, ROTATE, SCALE, MIRROR, ALIGN, grips, and DRAWORDER work without a case of their own. The pixels are not geometry: nothing snaps to what the picture shows. Its record holds its layer and tier like any other (ADR-0013, ADR-0032).

`Entity::table_refs` names the definition (#370). So `bring_in`, purge, and "is this row in use" follow the one rule. Removing the last image of a definition leaves the definition until PURGE, as a block's.

### 2. Images are a fourth committed instance kind, drawn beneath strokes

The committed scene gains **image quads**: four corners, the definition's index, and the fade, in the record's tier (ADR-0032). Images are drawn after fills and before strokes, write depth as fills do, and a stroke in the same tier lies over them. They go in the same chunks as everything else (ADR-0022): an edit re-uploads only its chunk.

**The pixels cross the boundary once per definition.** A coarse call returns a definition's bytes by row (ADR-0002). The adapter decodes them with the browser's decoder and uploads one texture per definition, kept until the definition's revision changes. Pointer movement, pan, and zoom never re-upload a texture (ADR-0003).

Rust never decodes an image. It reads only the pixel size from the header, to check the host's claim.

The overlay draws an image too, for the proposal's preview (§7). It is the same quad, in the overlay's own buffers, never committed.

This amends ADR-0003's three kinds and ADR-0021's "nothing else is drawn filled" as of this ADR. AGENTS.md says so when it lands.

### 3. The host makes every image the engine holds

An image enters the drawing only through the host (`HostRequest`, as files do):
- a PNG or JPEG the browser decodes;
- a page of a PDF rendered in the browser;
- a photo from a phone, as the browser gives it.

The host:
1. downsizes it to at most 2,048 pixels on its long side (a setting);
2. re-encodes it as JPEG, or as opaque 8-bit PNG for line art;
3. hands the engine the bytes.

So every image in a drawing is one the model could have seen, and one the PDF plot can embed without a decoder (§5).

**A PDF page** is rendered by pdf.js (Apache-2.0), loaded only when a PDF is chosen. The user picks the page; it is rendered at a resolution that meets the 2,048-pixel bound. A vector PDF's lines are not imported as geometry: a page is a picture here, and importing PDF geometry is another feature.

### 4. Files: the drawing carries its images

**DXF** writes each image as the standard `IMAGE`, with its `IMAGEDEF` and reactor. The `IMAGEDEF`'s file name is the image's name, so AutoCAD finds it beside the DXF if the user puts it there. It also writes **a copy of the bytes** in an `XRECORD` of a `CADSLATE_IMAGES` dictionary, as binary chunks (group 310), keyed by digest. This follows the `CADSLATE_CONSTRAINTS` precedent (ADR-0038).

So a DXF from CadSlate2D is self-contained here: the autosave, SAVE, and OPEN keep the image with no second file. Other programs pass the dictionary over and look for the file.

`SAVEAS` and `DXFOUT` can also save each image as a file, on request: the host asks once per export that holds images.

**A DXF from another program** names files the browser cannot open by path. Opening one shows each image's frame and name. An `IMAGEATTACH` option, Reload, lets the user pick the file, which then replaces the frame. The import report counts images whose file is missing.

**DWG** is out of this milestone. An image is reported as a loss when writing, and its frame and name are read. `docs/dwg-matrix.md` says so. Writing it is the same records through the DWG writer, later.

### 5. On paper

`PLOT` embeds each image in the PDF page, clipped to its frame and faded as drawn:
- a JPEG as it is (`DCTDecode`);
- a PNG's compressed data with its predictor (`FlateDecode`, `Predictor 15`).

Neither needs a decoder in Rust, which is why §3 fixes the formats. The `GeometrySink` walk gains an image call, so the screen and the plot share the walk (ADR-0027).

### 6. Commands

- `IMAGEATTACH` (alias `IAT`): choose a file through the host, then an insertion point, a scale (drawing units across the image's width), and a rotation. Classic prompts, typed or by pointer.
- `IMAGE`: lists the drawing's images and their definitions, with Reload and Detach.
- `IMAGEADJUST`: the fade.
- `IMAGEFRAME`: frames shown or hidden, for every image, as AutoCAD's 0 and 1.

Clipping (`IMAGECLIP`) and brightness or contrast are not in this milestone.

### 7. The AI: an image with the request, an `underlay` in the answer

**In the panel,** the user attaches an image or a PDF page to the next request. The host makes it as §3 says.

**Consent** shows the image with the rest of what will go: "This image will be sent to the AI provider with this request, and not stored by the service". Nothing is sent without that consent, as for drawing context (§43).

**The request** carries the image and its pixel size. The service sends it to the model with the words and never writes it to its database. Its record notes only that an image came with the request. What a plan allows is the service's (its repository).

**The model answers** with an intent document as for words. Intent version 27 adds one operation, `underlay`:

```json
{ "op": "underlay", "at": [x, y], "width": w, "rotation": 0, "fade": 50 }
```

It places the request's image with its lower-left corner at `at`, spanning `w` drawing units, so that the image lies under what the document draws as the image shows it. The model knows the image's pixel size and the sizes it read. The image itself is not in the document: `underlay` names the request's image, which the engine holds for the request.

**The proposal** shows the image under the drawn geometry in the overlay (§2). On Accept, the image definition, the image entity, and the geometry are committed in one transaction: one undo. Rejected, nothing of the image stays in the drawing.

**A drawing follows the sizes the image writes, never the pixels.** Where the image states no size and no scale — no figure, no scale bar, no ratio in the title — the right answer is a question (L6's rule, #365): "the sketch gives no size; how wide is the front wall?" A proportion read from pixels is allowed only where one written size scales the rest, and the reply says so.

The review and the requirements check run on the answer as on any (#349, #362). The model writes as requirements the sizes the image states.

### 8. L7: drawings from images

The corpus gains **L7**, in English and Portuguese, each case an image and a request. Its images are ours to commit:
- **Sketches generated** from drawings of our own, rendered with a hand-drawn stroke by a script in this repository, from a seed. The script and its inputs are committed, and the images are reproducible.
- **Scanned plans in the public domain:** the Historic American Buildings Survey's measured drawings at the Library of Congress, works of the United States government, each checked for its rights statement and its source recorded in `docs/provenance.md`.
- **The maintainer's own photographs** of hand sketches, if given.

A case is judged on what the image states:
- each written size is drawn, and dimensioned where asked;
- the structure the image shows: counts of closed outlines, of openings, of rooms;
- the underlay placed so that two named marks of the image fall within a tolerance of the points they mark in the drawing;
- the reply asks where the image gives no size.

A judged size is exact; a placement is within 2% of the image's width.

### 9. First, a spike

Before anything is built, a spike measures the configured model on twelve images:
- four generated sketches;
- four public-domain scans;
- two PDF pages;
- two images with no size written.

It measures how many written sizes it reads right, whether its outlines close, how close its `underlay` lands, and whether it asks where it should. The ADR's acceptance waits on it.

If the model reads written sizes reliably but places poorly, §7 stands and the placement becomes a reviewed finding. If it cannot read sizes, the AI half waits for a better model: the underlay (§1–§6) is worth building alone.

## Alternatives considered

- **The image in the AI panel only.** Simplest, and no engine change. Not chosen: the maintainer asked for the image in the drawing, and an underlay is how a drafter checks a traced drawing against its original.
- **The image as an overlay of the preview only.** No entity, no file format. Not chosen for the same reason. It survives as how the proposal is previewed (§7).
- **Tracing the image into vectors in Rust**: edges, then lines by a Hough transform, given to the model as geometry. Not now: it is a large, tunable piece whose value is unmeasured. Revisit if the spike shows the model places outlines poorly while reading sizes well.
- **Sidecar files beside the DXF,** as AutoCAD does. Not chosen as the only way: a browser cannot open a file by path, and the autosave is one DXF. The embedded copy keeps the drawing whole; the standard `IMAGEDEF` keeps it readable elsewhere.
- **Storing the image at the service for 30 days,** to debug failures. Declined by the maintainer: an image is the user's content, as a drawing is (§43).

## Consequences

- **cad-document:** the image-definition table and `Entity::Image`, with their transactions, history property test, and `table_refs`.
- **cad-render and the adapter:** a fourth committed instance kind, a texture per definition, and image quads in the overlay. A render-boundary test that pan and zoom upload no texture.
- **cad-dxf:** `IMAGE`, `IMAGEDEF`, its reactor, and `CADSLATE_IMAGES` read and written, byte-identical on a round trip. **cad-dwg:** frames read, images reported as losses.
- **cad-plot:** images embedded in the page.
- **cad-commands:** `IMAGEATTACH`, `IMAGE`, `IMAGEADJUST`, `IMAGEFRAME`, and the `underlay` operation's command. **cad-intent:** version 27.
- **web:** the attach and PDF-page pickers, pdf.js as a lazily loaded dependency, the panel's attachment and consent.
- **The service** (its repository): an image in the request, sent to the model, never stored, gated by plan; the privacy statement; `record.sh` and the nightly with L7's images.
- **The corpus:** L7, its generator script, and its provenance.

## Revisit when

- The spike or L7 shows sizes read and outlines placed poorly: weigh the vector-tracing pre-pass.
- Users ask for clipping, brightness, or contrast: `IMAGECLIP` and `IMAGEADJUST`'s other settings.
- DWG users meet images: write them through the DWG writer.
- A PDF's vector geometry is wanted as lines: a PDF import, by its own ADR.
