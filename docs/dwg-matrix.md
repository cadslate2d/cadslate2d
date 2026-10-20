# What the DWG reader and writer do

This page is the published list of what `cad-dwg` reads from a DWG file and what it writes into one, container by container and record by record (DESIGN §24, §40; [ADR-0036](adr/0036-dwg.md)). Anything not listed is reported by its kind through the same `ImportReport` the DXF reader fills; it is never guessed at, and never silently dropped (§24.2, LOCKED).

Every row names the test or fixture that backs it: a test by its name followed by parentheses, a fixture by its file name. A test (`the_matrix_names_only_tests_and_fixtures_that_exist()` in `crates/cad-dwg/tests/fixtures.rs`) fails when a name here does not exist, so a row cannot outlive what backs it. The fixtures are listed in [`test-data/dwg/README.md`](../test-data/dwg/README.md).

The rules that hold throughout:

- **Three versions, read and written alike.** `AC1015` (AutoCAD 2000), `AC1018` (2004), and `AC1021` (2007/2008). A later version is refused with an error that names it (`a_version_this_crate_does_not_read_is_named_in_the_error()`, `version-2010.dwg`); anything else is not a DWG (`the_three_versions_are_told_apart_and_the_rest_refused()`).
- **One set of record semantics.** The specification gives every field its DXF group code; the reader decodes each object by its prescription into those groups and hands them to `cad-dxf`'s reader as records (`cad_dxf::import_records`), so what a layer or a line means is written once. Every fixture makes the drawing its DXF source makes, entity by entity, property by property, row by row, in all three versions (`every_fixture_makes_the_drawing_its_dxf_source_makes()`; the fixtures are listed in [`test-data/dwg/README.md`](../test-data/dwg/README.md)).
- **Untrusted input** (§43). Every count a file can grow is bounded by a field of `ImportLimits`, each reached by a test; a page, a section, or an object is decoded within the bytes it has, into buffers that grow with what is produced; arbitrary bytes never panic (the `dwg_read` fuzz target, `scripts/fuzz.sh`).

## The containers

| | `AC1015` | `AC1018` | `AC1021` | Tests |
| --- | --- | --- | --- | --- |
| The version (§3.2.1) | The first six bytes | The same | The same | `the_three_versions_are_told_apart_and_the_rest_refused()` |
| The file header | Section locators at 0x15, the CRC over the header XORed by the locator count, the sentinel (§3.2.6) | 0x6c bytes at 0x80 XORed with the magic sequence, the `AcFssFcAJMB` id, the CRC-32 (§4.1) | 0x3d8 bytes at 0x80 de-interleaved from three Reed–Solomon (255, 239) blocks, decompressed when `ComprLen` is positive (§5.2) | `locators_are_read_and_the_header_crc_and_sentinel_checked()`, `the_magic_sequence_starts_as_the_specification_prints_it()`, `a_file_too_short_for_its_header_is_truncated_not_a_panic()`, `a_short_file_is_truncated_not_a_panic()`, `bad-header-crc.AC1015.dwg` |
| Where the sections are | The locators: 0 header, 1 classes, 2 the object map, 3 the unknown section, 4 the template | The page map and section map in system pages (§4.3–4.5), each page's checksum checked, each data page's masked header and two checksums checked (§4.6) | The page map and section map in system pages (§5.3), data pages de-interleaved from (255, 251) blocks when the page is that size (§5.4) | `every_valid_fixture_opens_with_its_sections_and_an_increasing_object_map()`, `bad-page-checksum.AC1018.dwg` |
| Compression | None | LZ77 (§4.7): literal lengths, long offsets, and the opcodes of the specification's examples | The R21 variant (§5.10), with the permuted literal copies of §5.10.1 | `the_specifications_literal_lengths_and_long_offsets()`, `a_short_opcode_copies_a_run_and_bad_input_is_refused()`, `the_2007_literal_copy_permutes_each_run()`, `a_2007_stream_of_one_literal_and_one_back_copy()` |
| Checksums | CRC-16 of §2.14.1 on the file header, the header section, and every object | CRC-32 of §2.14.2 on the file header; the §4.2 sum on every page | None checked: the 64-bit CRCs of §5.12 need the random encoder of the writer (#317) | `the_tables_are_the_specifications()`, `the_page_checksums_are_stable()` |
| Reed–Solomon | — | — | De-interleaved only, the block count taken from the page on disk, never from a claimed size; a damaged page is a checksum or decoding error, not corrected | `deinterleaving_keeps_the_data_bytes_of_each_block()`, `fuzz-huge-page-blocks.AC1021.dwg` |
| Truncation | A file that ends inside a section, a page, or an object is `Truncated` at the byte | The same | The same | `truncated.AC1015.dwg`, `truncated.AC1021.dwg`, `every_malformed_fixture_is_refused_with_the_error_its_name_says()`, `reading_past_the_end_is_an_error_at_the_byte()` |
| Limits (§43) | `ImportLimits`: 256 MB of file; 64 sections; 65,536 pages; 256 MB per decoded section; 4,000,000 objects; 16 MB per object; 64 KiB per string; 10,000 classes | | | `a_file_over_the_byte_limit_is_refused()`, `too_many_sections_are_refused()`, `too_many_pages_are_refused()`, `a_section_over_the_byte_limit_is_refused()`, `too_many_objects_are_refused()`, `an_object_over_the_byte_limit_is_refused()`, `a_string_over_the_byte_limit_is_refused()`, `too_many_classes_are_refused()` |

## The bit codes (§2)

| Code | Read as | Tests |
| --- | --- | --- |
| B, BB, 3B | Bits | `three_bit_counts_and_long_longs()` |
| BS, BL, BLL | The two-bit prefix forms of §2.2–2.4, on the specification's own examples | `five_bit_shorts_of_the_specifications_example()`, `four_bit_longs_of_the_specifications_example()` |
| BD, DD, BT, BE | §2.5, §2.9 (a default patched by four or six bytes), §2.10, §2.8 | `bit_doubles_defaults_thickness_and_extrusion()` |
| MC, MS | Modular chars and shorts (§2.6–2.7), on the specification's examples; the object map's unsigned form | `modular_chars_and_shorts_of_the_specifications_examples()` |
| H | Code, counter, bytes (§2.13), and the offset codes 6, 8, 0xA, 0xC resolved against a reference | `a_handle_of_the_specifications_example_and_its_resolution()` |
| T, TU | A bit-short length and bytes in the code page, or UTF-16 units from the string stream (R2007); terminating NULs dropped; bounded by `max_string_bytes` | `texts_in_both_kinds_with_their_limits()` |
| CMC, ENC | §2.11: an index; from R2004 an RGB value, a colour book handle in the handle stream, a transparency | `entity_colours_in_both_encodings()` |

## The sections

| Section | Read | Tests |
| --- | --- | --- |
| The object map (§23) | Sections of at most 2032 bytes of handle and offset deltas, closed by a section of size 2; offsets absolute in R2000, relative to the objects section after; an entry past the objects is left out and counted (`File::unreachable_objects`), since a file may list objects it no longer holds | `the_object_map_accumulates_handles_and_offsets()`, `every_valid_fixture_opens_with_its_sections_and_an_increasing_object_map()` |
| Classes (§10) | Each class's number, proxy flags, names, zombie flag, and item class id; R2004's maximum class number and counts; R2007's names from the string stream | `the_classes_of_every_fixture_name_their_dxf_kinds()` |
| Header variables (§9) | Every variable of the R2000 to R2007 sequence into a table by name, the handles from the handle stream and the texts from the string stream in R2007, `HANDSEED` always from the data; the CRC checked | `the_header_variables_of_every_fixture_name_the_spaces_and_the_current_layer()`, `the_layout_names_each_variable_once_and_holds_by_version()` |
| Objects (§20.1–20.2) | Each object's size, type, bit size, handle, extended data, and three streams; the string stream found from the bit before the handles (§20.1); the common entity data and handles of §20.4.1–20.4.2 and the common object data and handles of §20.1; the CRC after each object checked | `every_object_of_every_fixture_decodes_its_header_common_data_and_handles()`, `a_string_stream_is_found_from_the_bit_before_the_handles()`, `the_fixed_types_are_sorted_into_entities_and_objects()` |
| Encoding | R2000 and R2004 strings are bytes in the drawing's code page, read as UTF-8 and lossily where they are not; R2007 strings are UTF-16 from the string stream. A code page table is not applied yet | `texts_in_both_kinds_with_their_limits()` |
| Everything else (`AcDb:Preview`, `AcDb:SummaryInfo`, `AcDb:AppInfo`, `AcDb:FileDepList`, `AcDb:RevHistory`, `AcDb:ObjFreeSpace`, `AcDb:Template`, `AcDb:AuxHeader`, `AcDb:Security`, the second header of R2000) | Found and decoded as bytes, not read | `every_valid_fixture_opens_with_its_sections_and_an_increasing_object_map()` |

## Tables and blocks

Every row's name, flags, and handles; the handles a row names — a layer's linetype, a dimension style's text style — become names once every row is read. The header variables the drawing has settings for become the DXF header's `$INSUNITS`, `$CECOLOR`, `$CELWEIGHT`, `$LTSCALE`, `$PDMODE`, `$PDSIZE`, `$DIMSTYLE`, and `$TEXTSTYLE` (`$CLAYER` and `$CELTYPE` too, where the file carries them).

| Record | Read (spec §) | Fixtures |
| --- | --- | --- |
| `LAYER` (§20.4.54) | Name; frozen, off (the 2 bit, not "on"), locked; colour by index or true colour; lineweight by the index table of §20.4.1; linetype by handle | `ezdxf-r12-layers-linetypes.AC1015.dwg`, `ezdxf-r2010-frozen-layers.AC1018.dwg`, `ezdxf-r2010-properties.AC1021.dwg`, `ezdxf-r2010-true-colour.AC1018.dwg` |
| `LTYPE` (§20.4.58) | Name, description, pattern length, each dash with its shape flags; the text area skipped | `ezdxf-r12-layers-linetypes.AC1018.dwg`, `ezdxf-r2010-properties.AC1015.dwg` |
| `STYLE` (§20.4.56) | Name, the shape and vertical bits (in that order), fixed height, width factor, oblique angle, generation, fonts | `ezdxf-r2010-text-styles.AC1015.dwg`, `ezdxf-r12-text.AC1021.dwg` |
| `DIMSTYLE` (§20.4.68) | Name and every variable of R2000 to R2007 under its DXF code, the text style by handle | `ezdxf-r2010-dimstyles.AC1015.dwg`, `ezdxf-r2010-dimstyles.AC1021.dwg` |
| `BLOCK_HEADER` (§20.4.52) | Name, flags, base point; the entities it owns, in order: the first and last of a chain in R2000, the owned list in R2004 and later; the layout handle | `ezdxf-r12-blocks.AC1015.dwg`, `ezdxf-r12-blocks.AC1018.dwg`, `ezdxf-r2010-block-attributes.AC1021.dwg` |
| `APPID`, `VPORT`, `VIEW`, `UCS`, the control objects, dictionaries, `LAYOUT`, `GROUP`, `SORTENTSTABLE`, `XRECORD`, `MLEADERSTYLE` | Found, not read: #314 reads what the drawing holds of them | — |

## Entities

Each decoded by its prescription into the groups a DXF carries, with the common data of §20.4.1 (handle, colour, lineweight, linetype scale, invisibility, space) and the handles of §20.4.2 (layer, linetype, and the owner, which places an entity in its block); angles in radians become degrees where DXF has degrees.

| Entity | Read (spec §) | Fixtures |
| --- | --- | --- |
| `LINE`, `CIRCLE`, `ARC`, `POINT` | §20.4.21, §20.4.20, §20.4.18, §20.4.31 | `ezdxf-r12-three-lines.AC1015.dwg`, `ezdxf-r2018-two-lines.AC1021.dwg`, `ezdxf-r2010-mixed-content.AC1018.dwg`, `ezdxf-r2010-points.AC1015.dwg` |
| `ELLIPSE` | §20.4.39 | `ezdxf-r2010-mixed-content.AC1015.dwg` |
| `XLINE`, `RAY` | §20.4.42–20.4.43 | `ezdxf-r2010-construction-lines.AC1018.dwg` |
| `SOLID`, `TRACE`, `3DFACE` | §20.4.35, §20.4.36, §20.4.32 | — (reported by the DXF reader as it reports them from a file) |
| `LWPOLYLINE` | §20.4.85: the flags, constant width, vertices as deltas, bulges, widths | `ezdxf-r2010-polylines.AC1015.dwg`, `ezdxf-r2010-bulged-lwpolylines.AC1021.dwg`, `ezdxf-r2010-wide-polylines.AC1018.dwg` |
| `POLYLINE` with `VERTEX` and `SEQEND` | §20.4.16–20.4.17, §20.4.11–20.4.12: the chain from the first to the last vertex (R2000) or the owned list (R2004 and later) | `ezdxf-r12-polylines.AC1015.dwg`, `ezdxf-r12-bulged-polylines.AC1018.dwg`, `ezdxf-r12-wide-polylines.AC1021.dwg` |
| `TEXT` | §20.4.3: the data flags and what each leaves out | `ezdxf-r12-text.AC1015.dwg`, `ezdxf-r2010-text-styles.AC1018.dwg` |
| `MTEXT` | §20.4.46: the words in one string, cut into pieces of 250 as a file carries them; the direction as a vector | `ezdxf-r2010-mtext.AC1015.dwg`, `ezdxf-r2010-mtext.AC1021.dwg` |
| `INSERT`, `MINSERT` | §20.4.9–20.4.10: the scale by its flags, the block by handle, the attributes it owns (the owned count stored only when there are attributes) | `ezdxf-r12-blocks.AC1021.dwg`, `ezdxf-r2010-block-attributes.AC1015.dwg` |
| `ATTRIB`, `ATTDEF` | §20.4.4–20.4.5: a text with its tag, flags, and prompt | `ezdxf-r2010-block-attributes.AC1018.dwg` |
| `SPLINE` | §20.4.40: both scenarios, knots, control points with weights, fit points | `ezdxf-r2010-mixed-content.AC1021.dwg` (through #182's reader) |
| `HATCH` | §20.4.75: gradient data (R2004 and later), every path kind and edge kind, the pattern lines, the seed points, in the order a file carries them | `ezdxf-r2010-hatches.AC1015.dwg`, `ezdxf-r2010-hatches.AC1018.dwg` |
| `WIPEOUT` | §20.4.80, an image with no picture | `ezdxf-r2010-draw-order.AC1015.dwg` |
| `DIMENSION`, `LEADER`, `MULTILEADER`, `VIEWPORT` | Read in #314; until then counted as `DWG:DIMENSION` and the like | `crafted-proxy-entity.AC1015.dwg` |
| `ACAD_PROXY_ENTITY`, `MLINE`, `TOLERANCE`, `REGION`, `3DSOLID`, `BODY`, `SHAPE`, `OLEFRAME`, `OLE2FRAME`, meshes, images, and every class entity this crate does not decode | Counted by kind, never guessed at | `crafted-proxy-entity.AC1015.dwg` |
| An object of a class the drawing does not define | Counted as `(unknown-class-N)` | `crafted-unknown-class.AC1015.dwg` |
| An object that cannot be decoded | Counted as `(unreadable-dwg-object)` or by its kind; an object map entry past the file as `(dwg-object-missing-from-file)` | `every_malformed_fixture_is_refused_with_the_error_its_name_says()` |

## Where the specification is wrong

Found by the spike behind ADR-0036 and confirmed in LibreDWG's source; the code follows the files.

| Where | What the specification says | What the files do |
| --- | --- | --- |
| §20.4.1, the entity's "nolinks" bit | "For R2004+ this always has value 1" | The bit is not stored in R2004 and later |
| §2.11, ENC's RGB value | "a BS containing the RGB value" | A BL |
| §23.1, an object map section's size | "size of this section", without saying what it counts | It counts its own two bytes; the last section's 2 is just the size |
| §5.10.1, the literal copy's byte order | The table is given without saying it applies to every run | It applies to every literal run; a straight copy produces nonsense |
| §20.3, DICTIONARY at 0x2A | Listed between entities | An object, not an entity; `fixed_type_is_entity()` says so |
| §2.11, CMC in R2004 and later | "BL: RGB value" | The long's high byte is a method: 0xC0 by layer, 0xC1 by block, 0xC2 a true colour, 0xC3 an index in the low byte |
| §20.4.54, the layer's "on" bit | "on (2 bit)" | The bit means off |
| §20.4.54 and every table row, the flag bits around the xref index | "64-flag B, xrefindex+1 BS, Xdep B" | In R2007 only the bit short is stored |
| §20.4.56, the text style's flag bits | "Vertical B, shape file B" | The shape bit comes first |
| §20.4.9, the insert's owned-object count | Listed for R2004 and later | Stored only when the insert has attributes |
