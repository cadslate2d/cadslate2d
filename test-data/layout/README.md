# Floor-plan briefs

The corpus of briefs for the layout search (ADR-0039, #277, #278): one per line, each a `name`, a `spec` (the brief, `cad_layout::Spec`, in metres), and what should come of it:

- `expect`:
  - `feasible`: a plan is made, and `verify` passes it;
  - `hard`: a brief at the edge of feasibility, made most of the time from any seed; a verified plan or an exhausted search, never anything else;
  - `infeasible`: refused before any search, by a proof;
  - `exhausted`: no plan found in the search's budget, nothing proved.
- `reason`: a phrase the refusal's reasons must contain.
- `note`: why the brief is there, where that is not plain.

`crates/cad-layout/tests/briefs.rs` holds every brief to its expectation in `make check`. Two examples measure the search:

```bash
cargo run --release -p cad-layout --example briefs
```

runs each brief once, and draws its plans as SVG with `SVG=DIR`;

```bash
cargo run --release -p cad-layout --example reliability
```

solves each brief from 100 starting seeds and prints the table ADR-0039 records: how often each comes out as expected, how long it takes, and how often circulation was added (`CIRCULATION=0` forbids it).
