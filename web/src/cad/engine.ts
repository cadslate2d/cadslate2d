// The only module that imports the generated WASM package (ADR-0002, ADR-0004).
import init, { core_version } from "./pkg/cad_wasm";

let ready: Promise<void> | undefined;

/** Instantiates the WASM module once; later calls share the same promise. */
export function loadEngine(): Promise<void> {
  ready ??= init().then(() => undefined);
  return ready;
}

/** Version of the Rust core. Call only after `loadEngine()` resolves. */
export function coreVersion(): string {
  return core_version();
}
