/** Compiles and links a shader program, failing loudly with the driver's log. */
export function linkProgram(
  gl: WebGL2RenderingContext,
  vertexSource: string,
  fragmentSource: string,
): WebGLProgram {
  const compile = (type: number, source: string): WebGLShader => {
    const shader = gl.createShader(type);
    if (!shader) throw new Error("could not create shader");
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      throw new Error(
        `shader compilation failed: ${gl.getShaderInfoLog(shader) ?? "unknown error"}`,
      );
    }
    return shader;
  };

  const program = gl.createProgram();
  const vertex = compile(gl.VERTEX_SHADER, vertexSource);
  const fragment = compile(gl.FRAGMENT_SHADER, fragmentSource);
  gl.attachShader(program, vertex);
  gl.attachShader(program, fragment);
  gl.linkProgram(program);
  // The program keeps the compiled code; the shader objects can go.
  gl.deleteShader(vertex);
  gl.deleteShader(fragment);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(`shader link failed: ${gl.getProgramInfoLog(program) ?? "unknown error"}`);
  }
  return program;
}
