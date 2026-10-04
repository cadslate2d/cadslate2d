import { useEffect, useRef, useState } from "react";
import { CanvasController } from "../app/canvas-controller";
import { CadEngine } from "../cad/engine";
import { webGl2Available } from "../renderer/webgl/support";

/**
 * The model-space canvas. It owns the engine session and its controller for
 * as long as it is mounted; React itself holds no drawing data.
 */
export function CanvasView() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [supported] = useState(webGl2Available);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const engine = new CadEngine();
    const controller = new CanvasController(canvas, engine);
    return () => {
      controller.dispose();
      engine.dispose();
    };
  }, []);

  return (
    <section className="canvas-region" aria-label="Model space">
      {supported ? (
        <canvas ref={canvasRef} className="canvas-region__canvas" tabIndex={0} />
      ) : (
        <p className="canvas-region__message canvas-region__message--error" role="alert">
          WebGL2 is not available in this browser, so the drawing area cannot be shown.
        </p>
      )}
    </section>
  );
}
