# Generated benchmark drawings

Drawings here are produced by `cad-benchmarks` (the crate one directory up) from a fixed seed, so every machine gets the same file. The `.dxf` outputs are git-ignored.

```bash
cargo run --release -p cad-benchmarks --bin generate-drawing -- 100000 benches/generated-drawings/lines-100k.dxf
```

The generator spreads lines over a square whose side grows with the square root of the count, so density is the same at every size. Nine in ten lines are short (1–20 units); the rest are long (50–500 units) and cross many others.

Benchmarks:

```bash
cargo bench -p cad-benchmarks
```
