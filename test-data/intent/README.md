# Intent documents

The intent IR (ADR-0037, #253) is what a model may write to change a
drawing: JSON of a known version, a unit, and operations from the
catalogue in `crates/cad-intent/src/lib.rs`.

| File | What |
| --- | --- |
| `schema-v15.json` | The JSON Schema of version 15 (version 1, the parts of #272, the placements of #273, the constraints and parameters of #274, the floor plans of #282, their windows of #324, `erase` of #325, `dim_scale` of #327, `members` of #329, `stair` with a brief's stair room and storey height of #331, the isometric operations of #336, `elevation` with a brief's `building` of #337, `building_section` of #338, `part_views` of #339, `title_block` of #340, `spline` and `hatch` of #345, and `view` of #351), generated from the Rust types by `cad_intent::schema()` and snapshot-tested (`the_schema_is_the_recorded_one_or_the_version_is_bumped()`). A change to the types changes the schema: bump `cad_intent::VERSION`, re-record with `UPDATE_SNAPSHOTS=1 cargo test -p cad-intent`, and name the file after the new version. |
| `example-plate.json` | A document of every kind of operation a plate needs, a seed for the `intent_parse` fuzzer. |

A document of an earlier version is still read (`cad_intent::READS`): each version is a subset of the next.
