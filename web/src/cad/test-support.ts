// Loads the generated WASM module from disk so engine tests can run in Node.
import { readFile } from "node:fs/promises";
import { loadEngine } from "./engine";

export async function loadEngineForTests(): Promise<void> {
  const bytes = await readFile(new URL("./pkg/cad_wasm_bg.wasm", import.meta.url));
  await loadEngine(bytes);
}
