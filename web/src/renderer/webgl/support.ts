let cached: boolean | undefined;

/** Whether this browser can create a WebGL2 context. Probed once. */
export function webGl2Available(): boolean {
  if (cached === undefined) {
    const gl = document.createElement("canvas").getContext("webgl2");
    cached = gl !== null;
    // Free the probe context immediately; browsers cap live contexts.
    gl?.getExtension("WEBGL_lose_context")?.loseContext();
  }
  return cached;
}
