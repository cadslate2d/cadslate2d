// Draw › Hatch... (#346): the pattern, its angle, and its scale chosen in a
// dialog, as AutoCAD's Hatch and Gradient dialog chooses them, then the
// boundary picked by a point inside it or by its objects. Each pattern is
// shown by its swatch, the lines the engine's own hatch draws over a square.
// The dialog only types: HATCH with its Pattern, Angle, and SCale options,
// a macro over the command line (ADR-0034), and HATCH remembers the choice.
import { useEffect, useMemo, useState } from "react";
import type { CadApp } from "../app/cad-app";
import type { Macro } from "../app/macro";
import { hatchSwatches, type HatchSwatch } from "../cad/engine";

/** The macro that hatches with `look`, then asks for the boundary: a point, or (`select`) objects. */
export function hatchMacro(
  look: { pattern: string; angle: number; scale: number },
  select: boolean,
): Macro {
  return {
    command: "HATCH",
    steps: [
      "P",
      look.pattern,
      "A",
      String(look.angle),
      "SC",
      String(look.scale),
      ...(select ? ["S"] : []),
    ],
  };
}

function Swatch({ swatch }: { swatch: HatchSwatch }) {
  if (swatch.solid) {
    return (
      <svg className="hatch-dialog__swatch" viewBox="0 0 1 1" aria-hidden="true">
        <rect width="1" height="1" fill="currentColor" />
      </svg>
    );
  }
  const lines = [];
  for (let i = 0; i + 3 < swatch.lines.length; i += 4) {
    const [x0, y0, x1, y1] = swatch.lines.slice(i, i + 4) as [number, number, number, number];
    // y up in the drawing, down on the page.
    lines.push(<line key={i} x1={x0} y1={1 - y0} x2={x1} y2={1 - y1} />);
  }
  return (
    <svg className="hatch-dialog__swatch" viewBox="0 0 1 1" aria-hidden="true">
      <g stroke="currentColor" strokeWidth="0.02" strokeLinecap="round">
        {lines}
      </g>
    </svg>
  );
}

export function HatchDialog({ app, onClose }: { app: CadApp; onClose: () => void }) {
  const swatches = useMemo(() => hatchSwatches(), []);
  const [look, setLook] = useState(() => {
    const ui = app.ui.getSnapshot();
    return { pattern: ui.hatchPattern, angle: ui.hatchAngle, scale: ui.hatchScale };
  });
  const [angle, setAngle] = useState(String(look.angle));
  const [scale, setScale] = useState(String(look.scale));
  const angleValue = Number(angle);
  const scaleValue = Number(scale);
  const valid = angle.trim() !== "" && Number.isFinite(angleValue) && scaleValue > 0;
  const solid = swatches.find((s) => s.name === look.pattern)?.solid ?? false;

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  const run = (select: boolean) => {
    if (!valid) return;
    onClose();
    app.runMacro(hatchMacro({ ...look, angle: angleValue, scale: scaleValue }, select));
  };

  return (
    <div className="hatch-dialog" role="dialog" aria-label="Hatch">
      <p className="hatch-dialog__title">Hatch</p>
      <div className="hatch-dialog__patterns" role="radiogroup" aria-label="Pattern">
        {swatches.map((swatch) => (
          <button
            key={swatch.name}
            type="button"
            role="radio"
            aria-checked={swatch.name === look.pattern}
            aria-label={swatch.name}
            title={`${swatch.name}: ${swatch.description}`}
            className="hatch-dialog__pattern"
            onClick={() => setLook({ ...look, pattern: swatch.name })}
            onDoubleClick={() => run(false)}
          >
            <Swatch swatch={swatch} />
            <span className="hatch-dialog__name">{swatch.name}</span>
          </button>
        ))}
      </div>
      <p className="hatch-dialog__description">
        {swatches.find((s) => s.name === look.pattern)?.description}
      </p>
      <form
        className="hatch-dialog__fields"
        onSubmit={(event) => {
          event.preventDefault();
          run(false);
        }}
      >
        <label>
          Angle
          <input
            type="number"
            step="any"
            value={angle}
            disabled={solid}
            onChange={(event) => setAngle(event.target.value)}
          />
        </label>
        <label>
          Scale
          <input
            type="number"
            step="any"
            min="0"
            value={scale}
            disabled={solid}
            onChange={(event) => setScale(event.target.value)}
          />
        </label>
        {!valid && (
          <p className="hatch-dialog__error" role="alert">
            The angle is a number and the scale greater than zero.
          </p>
        )}
        <div className="warning__actions">
          <button type="button" className="text-editor__button" onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className="text-editor__button"
            disabled={!valid}
            onClick={() => run(true)}
          >
            Select Objects
          </button>
          <button
            type="submit"
            className="text-editor__button text-editor__button--primary"
            disabled={!valid}
          >
            Pick Points
          </button>
        </div>
      </form>
    </div>
  );
}
