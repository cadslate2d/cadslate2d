# What the DXF reader and writer do

This page is the published list of what the reader takes from a DXF file and what the writer puts into one, record by record (DESIGN §24, [ADR-0005](adr/0005-dxf-baseline.md), [ADR-0018](adr/0018-dxf-release-r2000.md)). Anything not listed is counted by its type name and reported; it is never guessed at, and never silently dropped (§24.2, LOCKED).

Every row names the test or fixture that backs it: a test by its name followed by parentheses, a fixture by its file name. A test (`the_matrix_names_only_tests_and_fixtures_that_exist()` in `crates/cad-dxf/tests/fixtures.rs`) fails when a name here does not exist, so a row cannot outlive what backs it. The fixtures themselves are listed in [`test-data/dxf/README.md`](../test-data/dxf/README.md).

Two rules hold throughout:

- **The reader is release-agnostic.** It reads group pairs and looks only at what is listed here, so a file of any release — R12 through R2018 are in the fixtures — imports, and a group it does not know is skipped. Handles, owners, and subclass markers are never read.
- **The writer writes one release, R2000 (`AC1015`)**, complete: every record with a handle and an owner, the nine tables, both spaces, and the objects that tie them together. Export → import → export is byte-identical for every fixture (`every_valid_fixture_round_trips_stably()`) and every file of the round-trip corpus, whose sidecars say what each must import as (`every_corpus_file_imports_as_its_sidecar_says_and_round_trips()`, [`test-data/dxf/README.md`](../test-data/dxf/README.md#corpus)).

## The file

| | Read | Written | Tests |
| --- | --- | --- | --- |
| Release | Any ASCII release | R2000 | `a_complete_r12_file()`, `a_modern_file_keeps_every_significant_digit()`, `an_empty_drawing_is_a_complete_file_with_its_permanent_rows()` |
| Binary DXF | Refused: "save as ASCII DXF" | — | `binary_dxf_is_rejected_with_a_clear_message()`, `binary-sentinel.dxf` |
| Encoding | Bytes decoded as UTF-8, lossily: a name that does not decode is still a name | ASCII, `$DWGCODEPAGE` `ANSI_1252` | `non_utf8_bytes_do_not_stop_an_import()` |
| Line endings, padding, number forms | LF or CRLF; padded codes; exponents and leading dots | LF; codes right-aligned in three columns; the shortest decimal that reads back exactly, always with a point | `tolerates_crlf_padding_exponents_and_missing_sections()`, `crlf-exponent-notation.dxf`, `reals_always_carry_a_decimal_point()`, `formatted_reals_parse_back_exactly()` |
| Limits (§43) | Every quantity a file can grow is bounded by an `ImportLimits` field: 256 MB of file; 2,000,000 entities, in the drawing and in blocks together; 100,000 rows per table, the file's and those made for undefined names (past it, a defined row is an error and a name that would make one is treated as one no row can have); 1,000,000 groups in one record; 64 KiB in one value; 32 levels of block nesting (deeper is reported, `(blocks nested too deep)`). A type name is kept to 32 printable characters; the unsupported-kinds report keeps 64 distinct names. Reaching a bound is an error or a report line, never an allocation in proportion to a claimed count — a polyline's count (90) is never believed | — | `limits_are_enforced()`, `a_table_with_more_rows_than_the_limit_is_refused()`, `rows_made_for_undefined_names_stop_at_the_limit_and_are_reported()`, `a_record_with_more_groups_than_the_limit_is_refused()`, `a_value_longer_than_the_limit_is_refused()`, `blocks_nested_deeper_than_the_limit_are_reported()`, `a_flood_of_empty_blocks_stops_at_the_row_limit()`, `a_polyline_claiming_billions_of_vertices_imports_the_two_it_has()`, `hostile_type_names_are_sanitised()`, `distinct_unsupported_kinds_are_bounded()`, `long-value.dxf`, `huge-vertex-count.dxf` |
| Time | The reader's work is proportional to the bytes read: tables are indexed by name, and nothing is re-scanned per record. Ten megabytes of groups import within a minute in a debug build; a hundred within a minute in release, run by `scripts/stress.sh` | — | `ten_megabytes_of_groups_import_in_bounded_time()`, `a_hundred_megabytes_of_groups_import_within_the_budget()` |
| Fuzz | Files of every record kind with any groups never panic, and whatever they import survives export → import → export; `scripts/stress.sh` runs it with many more cases | — | `arbitrary_bytes_never_panic()`, `arbitrary_dxf_shaped_text_never_panics()`, `arbitrary_records_of_every_kind_never_panic_and_round_trip()`, `arbitrary_tables_and_properties_never_panic()` |
| Sections | `HEADER`, `TABLES`, `BLOCKS`, `ENTITIES`; everything else skipped | `HEADER`, `CLASSES` (empty), `TABLES`, `BLOCKS`, `ENTITIES`, `OBJECTS` | `a_file_with_no_entities_section_is_an_empty_drawing()`, `an_empty_drawing_is_a_complete_file_with_its_permanent_rows()` |
| Order | Entities and blocks in file order; names resolved once the file is read, so a table may follow what uses it and a block may place one defined after it | Entities in `EntityId` order; handles in writing order | `output_is_deterministic_and_ordered_by_entity_id()`, `names_the_file_uses_but_never_defines_are_created_and_reported()`, `nested_blocks_from_another_writer()` |

## Header variables

Everything else in the header is skipped.

| Variable | Read as | Written when | Tests |
| --- | --- | --- | --- |
| `$ACADVER` | Nothing: the release does not change what is read | Always: `AC1015` | `a_complete_r12_file()` |
| `$DWGCODEPAGE` | Nothing | Always: `ANSI_1252` | `an_empty_drawing_is_a_complete_file_with_its_permanent_rows()` |
| `$INSUNITS` (70) | The drawing unit: 1 inches, 2 feet, 4 millimetres, 5 centimetres, 6 metres; 0 or absent is unitless; any other code is reported and the drawing is unitless | The drawing has a unit | `the_drawing_unit_is_read_from_the_header()`, `a_file_without_a_unit_or_with_unit_zero_is_unitless()`, `a_unit_the_drawing_cannot_be_in_is_reported_and_the_drawing_is_unitless()`, `the_drawing_unit_survives_a_round_trip_and_a_unitless_file_is_plain_r12()` |
| `$CLAYER` (8) | The current layer, if the file defines it and it is on | Not layer `0` | `the_header_sets_the_current_layer_colour_linetype_lineweight_and_scale()`, `the_header_carries_the_settings_that_are_not_a_new_drawings()` |
| `$CECOLOR` (62) | The colour for new objects | Not `ByLayer` | the same |
| `$CELTYPE` (6) | The linetype for new objects, if the file defines it | Not `ByLayer` | the same |
| `$CELWEIGHT` (370) | The lineweight for new objects | Not `ByLayer` | the same |
| `$LTSCALE` (40) | The linetype scale, if finite and positive | Not 1 | the same |
| `$EXTMIN`, `$EXTMAX` | Nothing | The drawing has anything in it | `a_line_is_written_with_its_handle_owner_layer_and_coordinates()` |
| `$HANDSEED` | Nothing | Always: the first free handle | `an_empty_drawing_is_a_complete_file_with_its_permanent_rows()` |

## Tables

| Table | Read | Written | Tests |
| --- | --- | --- | --- |
| `LAYER` | Name (2); colour (62), negative for a layer that is off; frozen (70 bit 1) as off; locked (70 bit 4); linetype (6), `ByLayer`/`ByBlock` as `Continuous`; lineweight (370); a true colour (420) as the nearest index, reported. A name no layer can have is reported. The first row of a name counts; `0` restyles the permanent row | Every layer: name, flags, colour (negative when off), linetype, lineweight (`-3` for the default), the shared plot style (390) | `the_layer_and_linetype_tables_are_read()`, `a_frozen_layer_is_off_and_a_lineweight_on_a_layer_is_kept()`, `layers_colours_and_linetypes_from_another_writer()`, `the_tables_are_written_with_patterns_switches_lineweights_and_linetypes()`, `ezdxf-r12-layers-linetypes.dxf` |
| `LTYPE` | Name (2), description (3), dash and gap lengths (49), normalised to pairs from a dash; a pattern the drawing cannot hold is reported and the linetype is continuous; text and shapes (74) are reported and the dashes kept. The `ByLayer`/`ByBlock` rows newer files carry are not linetypes. `Continuous` is continuous whatever a file says | `ByBlock`, `ByLayer`, then every linetype: name, description, alignment (72), count (73), period (40), each element (49) with its type (74) | `a_complex_or_overlong_pattern_is_reported_and_the_dashes_kept()`, `lineweights_scales_and_undefined_names_from_another_writer()`, `the_tables_are_written_with_patterns_switches_lineweights_and_linetypes()` |
| `STYLE` | Its name: that is all the drawing keeps of a text style. `Standard` restyles the permanent row | Every text style: name, no fixed height, width factor 1, the stroke font `txt` | `text_and_styles_from_another_writer()`, `a_text_style_other_than_standard_is_named_on_its_texts()`, `ezdxf-r12-text.dxf` |
| `BLOCK_RECORD` | Nothing: blocks are read from `BLOCKS` | `*Model_Space` and `*Paper_Space`, pointing at their layouts; one row per block | `blocks_are_written_as_records_definitions_and_inserts_and_survive_a_round_trip()` |
| `VPORT` | Nothing | One `*Active` viewport centred on the drawing's extents | `the_viewport_looks_at_the_whole_drawing()` |
| `VIEW`, `UCS`, `DIMSTYLE` | Nothing | Empty | `an_empty_drawing_is_a_complete_file_with_its_permanent_rows()` |
| `APPID` | Nothing | `ACAD` | the same |

A layer, linetype, or text style that an entity or the header names but no table defines is made — a new layer's settings, a continuous linetype, a style with that name — and the import report says so (`names_the_file_uses_but_never_defines_are_created_and_reported()`).

## Blocks

| Record | Read | Written | Tests |
| --- | --- | --- | --- |
| `BLOCK` … `ENDBLK` | A definition: name (2), base point (10, 20), and the entities between, read like the drawing's own. `*Model_Space`, `*Paper_Space`, `$MODEL_SPACE`, `$PAPER_SPACE`, anonymous `*…` blocks, a name no block can have, and a second definition of a name are skipped with their contents | `*Model_Space` and `*Paper_Space`, empty; then every definition, its entities owned by its block record | `user_blocks_are_imported_and_structural_ones_are_not()`, `structural_block_names_are_recognised_in_every_spelling()`, `nested_blocks_from_another_writer()`, `ezdxf-r12-blocks.dxf` |
| A cycle | Refused: "the file's tables cannot be held: block … would contain itself" | Cannot happen: a cycle is refused when a block is defined | `cyclic-blocks.dxf`, `malformed_fixtures_fail_for_the_documented_reason()` |

## Entities

Each entity's geometry, then what is read and written for it. Every entity also carries the [properties](#entity-properties) below.

| Entity | Read | Written | Tests |
| --- | --- | --- | --- |
| `LINE` | Start (10, 20) and end (11, 21); a missing coordinate is an error | The same, with Z 0 | `imports_a_line()`, `a_line_is_written_with_its_handle_owner_layer_and_coordinates()`, `minimal-line.dxf`, `line-missing-endpoint.dxf` |
| `CIRCLE` | Centre (10, 20), radius (40); a radius that is not positive is reported, not imported | The same | `imports_circles_and_arcs_with_angles_converted_from_degrees()`, `curves_with_a_radius_that_is_not_positive_are_reported_not_imported()`, `circles_and_arcs_from_another_writer()` |
| `ARC` | Centre, radius, start and end angles (50, 51) in degrees counter-clockwise | The same | `circles_and_arcs_are_written_with_angles_in_degrees()`, `circles_and_arcs_survive_a_round_trip()` |
| `LWPOLYLINE` | Vertices (10, 20 repeated), closed (70 bit 1), a bulge (42) after the vertex that starts its arc segment; a constant width (43) is dropped and reported; fewer than two vertices is reported | Every polyline: count (90), closed flag, vertices with their bulges | `imports_lwpolylines_from_their_repeated_coordinates()`, `polylines_with_arc_segments_keep_their_bulges()`, `a_polyline_is_written_as_an_lwpolyline()`, `bulges_are_written_as_group_42_after_their_vertices()`, `ezdxf-r2010-bulged-lwpolylines.dxf` |
| `POLYLINE`, `VERTEX`, `SEQEND` | The R12 form: a header, a vertex per record with its bulge, an end; 3D polylines, meshes, and polyface meshes (70 bits 8, 16, 32, 64) are reported and their vertices swallowed | Never: a polyline is an `LWPOLYLINE` | `imports_r12_polylines_from_their_vertex_records()`, `bulged_polylines_in_both_encodings()`, `ezdxf-r12-bulged-polylines.dxf`, `ezdxf-r2010-polylines.dxf` |
| `TEXT` | First point (10, 20), height (40), words (1) with `%%d`, `%%p`, `%%c` as their signs, rotation (50), style (7), justification (72, 73) with the alignment point (11, 21): left, centre, right as they are; middle, top, bottom placed by moving the baseline; aligned and fit read as left at the first point and reported | Every text: first point, height, words, rotation when not 0, style when not `Standard`, and for centred and right text the justification and alignment point | `text_and_styles_from_another_writer()`, `a_text_is_written_with_its_insertion_height_words_angle_and_justification()`, `ezdxf-r12-text.dxf` |
| `MTEXT` | Insertion (10, 20), height (40), width (41), attachment point (71) placing the top left, rotation (50) or direction (11, 21), the words in pieces (3, then 1) with `\P` a break, `\~` a space, a stacked fraction as `a/b`, every other code dropped and reported | Every paragraph: top left, height, width, attachment 1, style when not `Standard`, the words with `\P` for a break and `\`, `{`, `}` escaped, in pieces of 250, rotation when not 0 | `paragraphs_from_another_writer()`, `a_paragraph_is_written_as_mtext_with_breaks_and_codes_escaped_and_survives()`, `ezdxf-r2010-mtext.dxf` |
| `INSERT` | Block (2), insertion (10, 20), scales (41, 42; 43 ignored), rotation (50). Equal magnitudes with one sign flipped are a mirror — a negative Y scale directly, a negative X scale as the mirror and a half turn; unequal magnitudes come in at the X magnitude and are reported. Column or row counts above one (70, 71) and attributes (66) are reported and the one reference imported. A block the file never defines is reported and the reference dropped; a scale of zero is reported | Every reference: block, insertion, scales when not 1 (a mirrored reference's Y negated), rotation when not 0 | `nested_blocks_from_another_writer()`, `block_attributes_are_reported()`, `blocks_are_written_as_records_definitions_and_inserts_and_survive_a_round_trip()`, `ezdxf-r2010-block-attributes.dxf` |
| `ATTRIB` | Skipped without a count of its own: its `INSERT` is reported as having attributes | Never | `block_attributes_are_reported()` |
| Anything else | Counted by its type name and reported | Never | `unsupported_entities_are_counted_by_kind_and_supported_ones_still_import()`, `unsupported_content_is_counted_and_reported()` |

## Entity properties

| Group | Read | Written | Tests |
| --- | --- | --- | --- |
| 8 layer | By name; a layer the file lacks is made; a name no layer can have puts the entity on `0` and is reported | Always | `names_the_file_uses_but_never_defines_are_created_and_reported()`, `an_entity_carries_its_layer_and_its_own_colour_linetype_and_lineweight_only()` |
| 6 linetype | By name: `BYLAYER`, `BYBLOCK`, or a row, made continuous if the file lacks it | When not `ByLayer` | the same |
| 62 colour | 256 `ByLayer`, 0 `ByBlock`, 1–255 an index; a negative index is the index (an old file's "off"); anything else is dropped and reported | When not `ByLayer` | `properties_the_drawing_cannot_hold_are_reported()`, and the same |
| 420 true colour | The nearest index colour, reported; the 62 beside it is not trusted | Never: the drawing has no true colour | `a_true_colour_becomes_the_nearest_index_and_is_reported()`, `true_colours_from_another_writer()`, `ezdxf-r2010-true-colour.dxf` |
| 370 lineweight | `-1` `ByLayer`, `-2` `ByBlock`, `-3` default, or one of the standard widths in hundredths of a millimetre; anything else is dropped and reported | When not `ByLayer` | `lineweights_scales_and_undefined_names_from_another_writer()`, `everything_the_drawing_holds_survives_export_import_export_byte_for_byte()` |
| 48 linetype scale | Dropped and reported unless exactly 1 | Never | `properties_the_drawing_cannot_hold_are_reported()` |
| 30, 31 elevation; 39 thickness | Dropped and reported unless 0 | Z written as 0 | the same |
| 5, 330, 100 | Ignored | Always: handle, owner, subclass markers | `a_line_is_written_with_its_handle_owner_layer_and_coordinates()` |

## The import report

What the reader could not hold is counted under these keys and named in the command log after `OPEN`; `has_unsupported_content` is what makes the export warning appear (§24.2).

| Key | Counts |
| --- | --- |
| A type name (`HATCH`, `DIMENSION`, `SPLINE`, …) | An entity of a kind the drawing has no entity for |
| `POLYLINE` | A 3D polyline, mesh, or polyface mesh |
| `(invalid geometry)` | A circle or arc with a radius that is not positive; a polyline of fewer than two vertices; an `INSERT` with no block name or a scale of zero |
| `(layer name)`, `(linetype name)`, `(text style name)`, `(block name)` | A table row or block whose name no row can have |
| `(linetype pattern)` | A pattern the drawing cannot hold: the linetype is continuous |
| `(linetype shapes)` | Text or shapes in a complex linetype: the dashes are kept |
| `(layer true colour)` | A layer with a true colour: it has the nearest index |
| `(INSERT with a non-uniform scale)`, `(INSERT array)`, `(INSERT attributes)` | A reference imported as far as the model holds |
| `(INSERT of an undefined block)` | A reference the file gives no block for: not imported |
| `(blocks nested too deep)` | Blocks nested deeper than the 32 levels the drawing follows: what lies below is not drawn |

Beside the keys, the report counts the entities imported by kind, the entities whose properties were dropped (colour, lineweight, linetype scale, elevation, thickness, constant width, true colour, a layer or linetype name no row can have, aligned or fit text), the paragraphs whose formatting was dropped, the tables and the rows made for undefined names, and the unit (`skipped_entities_are_listed_by_kind()`, `tables_are_counted_and_rows_the_file_did_not_define_are_named()`).

## Errors

A file the reader cannot read at all is refused with one of these, each with the line it happened on (`malformed_files_return_structured_errors()`, `malformed_fixtures_fail_for_the_documented_reason()`).

| Error | When | Fixture |
| --- | --- | --- |
| Binary DXF | The file starts with the binary sentinel | `binary-sentinel.dxf` |
| File too large; too many entities; too many rows; record too large; value too long | Over a limit | `limits_are_enforced()`, `a_table_with_more_rows_than_the_limit_is_refused()`, `a_record_with_more_groups_than_the_limit_is_refused()`, `long-value.dxf` |
| Invalid group code | A code line that is not an integer | `not-dxf.dxf`, `bad-group-code.dxf` |
| Unexpected end | The file ends inside a group, a record, or a section, or without `EOF` | `empty.dxf`, `truncated-mid-entity.dxf`, `missing-eof.dxf` |
| Invalid number | A coordinate, radius, or angle that is not a finite number | `bad-coordinate.dxf` |
| Missing coordinate | An entity without a coordinate it must have | `line-missing-endpoint.dxf` |
| Tables | The file's tables cannot be held as a drawing: a cycle of blocks, a name taken twice | `cyclic-blocks.dxf` |

Not an error, ever: a group the reader does not know, a section it does not read, a record it does not import. Arbitrary bytes never panic (`degenerate_and_malformed_polylines()`, and the never-panics property test in `crates/cad-dxf/src/import.rs`).
