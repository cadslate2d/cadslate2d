# Fuzzing

Coverage-guided fuzzing of what takes input from outside (#177): the DXF reader, on bytes, and the input grammar, on any text at any kind of prompt. libFuzzer through `cargo fuzz`, which needs a nightly toolchain:

```bash
rustup toolchain install nightly && cargo install cargo-fuzz
scripts/fuzz.sh 600                 # both targets, ten minutes each
scripts/fuzz.sh 3600 dxf_import     # one target, an hour
```

Not part of `scripts/check.sh`. CI runs each target for twenty minutes weekly (`.github/workflows/fuzz.yml`) and on request, caching the corpus it grows between runs and keeping it as an artifact.

| Target | What it holds |
| --- | --- |
| `dxf_import` | `cad_dxf::import_with_limits` answers any bytes with a drawing or a structured error; what the writer writes for the result, the reader reads; and reading it again writes the same file — to floating-point precision, since what the file holds derived (degrees from radians, the points of a radius dimension from its radius) comes back an ulp off. The curated fixtures hold the stricter bar, byte for byte. Seeded with every DXF fixture. |
| `input_resolve` | `cad_input::resolve` on arbitrary text at every kind of prompt, with any of six keywords, with or without direct distance, in any unit or none, answers the same way twice and never panics; `parse_point` likewise. |

A crash lands in `fuzz/artifacts/<target>/` and becomes a fixture under `test-data/dxf/malformed/` or `valid/` (or a table row in `cad-input`'s tests) and a fix, in the same commit.

## Runs recorded

| Date | Target | Time | Executions | Coverage (edges) | Corpus | Found |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-10-12 | `dxf_import` | 45 s + 60 s | — | — | — | A diameter dimension's value written as `60.0` and, after one re-read, `59.99999999999999`: the radius recomputed from a far point the writer derived from it. Not a bug of the reader: the target's bar became float-exact. Then: a text ending in a space lost it on reading, and an MTEXT written in pieces fused the words at a piece boundary — every group value was trimmed. Fixed; `text-with-spaces.dxf`. |
| 2026-10-12 | `dxf_import` | 14 min, 10 min | — | — | — | Two more ulp drifts: a radius dimension's far point drifting by ulps of its radius (2e7), and a dimension's text — `Ø3217…688.00` then `…692.00` at a radius of 3e16. Not bugs; the target's tolerance became ulps of the file's largest magnitude, and numbers inside text compare as numbers. |
| 2026-10-12 | `dxf_import` | 25 min | — | — | — | **A hang:** a HATCH whose pattern line was placed at y = 1e20. The pattern's line indices were where one and the next are the same number, so the walk over them never moved; a spacing small enough to make the counts infinite would have hung the same way, as NaN passes `>`. `cad_geometry::pattern_lines` now refuses both, and the dash walk goes by index with a stop when the index cannot move; `hatch-pattern-origin-far.dxf`. |
| 2026-10-12 | `dxf_import` | 12 min | — | — | — | A `%` written as itself starts a `%%` code on reading: a run of eleven came back as three, and a text ending in `%%` lost it. The writer now writes `%%%`, the code for a `%`, in TEXT and MTEXT; `a_percent_sign_in_a_text_or_a_paragraph_survives()`. |
| 2026-10-12 | `dxf_import` | 1 h | DXF_EXECS | DXF_COV | DXF_CORP | DXF_FOUND |
| 2026-10-12 | `input_resolve` | 1 h | 149,661,685 (41,500/s) | 695 | 480 inputs | Nothing: no panic, no nondeterminism. |
