# Generated benchmark drawings

Drawings here are produced by `cad-benchmarks` (the crate one directory up) from a fixed seed, so every machine gets the same file. The `.dxf` outputs are git-ignored.

```bash
cargo run --release -p cad-benchmarks --bin generate-drawing -- 100000 benches/generated-drawings/lines-100k.dxf
```

The generator spreads lines over a square whose side grows with the square root of the count, so density is the same at every size. Nine in ten lines are short (1–20 units); the rest are long (50–500 units) and cross many others.

`--mixed` writes a drawing of every kind of entity in a drafter's proportions — per hundred: 44 lines, 8 circles, 8 arcs, 10 polylines (one in three with an arc segment), 8 texts, 2 paragraphs, 8 references to one of four blocks, 8 dimensions (linear, aligned, radius), 4 hatches (a square with a round hole, every other one `ANSI31`); `cad_benchmarks::MIXED_PER_HUNDRED` is the table. These are what the §42 targets are measured on (`docs/performance.md`, #171):

```bash
for n in 10000 50000 100000; do
  cargo run --release -p cad-benchmarks --bin generate-drawing -- --mixed $n benches/generated-drawings/mixed-$((n/1000))k.dxf
done
```

Benchmarks:

```bash
cargo bench -p cad-benchmarks
```
