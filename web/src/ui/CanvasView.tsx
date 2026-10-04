import { useEffect, useRef, useState } from "react";
import type { CadApp } from "../app/cad-app";
import { CanvasController } from "../app/canvas-controller";
import { webGl2Available } from "../renderer/webgl/support";

/** The model-space canvas. React holds no drawing data; it only hosts the element. */
export function CanvasView({ app }: { app: CadApp }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [supported] = useState(webGl2Available);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const controller = new CanvasController(canvas, app);
    return () => controller.dispose();
  }, [app]);

  return (
    <section className="canvas-region" aria-label="Model space">
      {supported ? (
        <canvas ref={canvasRef} className="canvas-region__canvas" />
      ) : (
        <p className="canvas-region__message canvas-region__message--error" role="alert">
          WebGL2 is not available in this browser, so the drawing area cannot be shown.
        </p>
      )}
    </section>
  );
}
