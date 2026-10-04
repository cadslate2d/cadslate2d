import type { ReactNode } from "react";
import type { CadApp } from "../app/cad-app";

interface Tool {
  /** The command name handed to the engine, exactly as if typed. */
  readonly command: string;
  readonly label: string;
  readonly icon: ReactNode;
}

/** 24×24 line icons in the style of the visual identity reference. */
const stroke = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.5,
  strokeLinecap: "round",
} as const;
const icon = (children: ReactNode) => (
  <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" {...stroke}>
    {children}
  </svg>
);

const DRAW: readonly Tool[] = [
  { command: "LINE", label: "Line", icon: icon(<path d="M4 20 20 4" />) },
  {
    command: "PLINE",
    label: "Polyline",
    icon: icon(<path d="M3 18 8 7l7 8 6-10" strokeLinejoin="round" />),
  },
  { command: "CIRCLE", label: "Circle", icon: icon(<circle cx="12" cy="12" r="8" />) },
  { command: "ARC", label: "Arc", icon: icon(<path d="M4 18A14 14 0 0 1 20 6" />) },
  {
    command: "RECTANG",
    label: "Rectangle",
    icon: icon(<rect x="4" y="6" width="16" height="12" />),
  },
];

const MODIFY: readonly Tool[] = [
  {
    command: "MOVE",
    label: "Move",
    icon: icon(
      <path d="M12 3v18M3 12h18M12 3l-3 3M12 3l3 3M12 21l-3-3M12 21l3-3M3 12l3-3M3 12l3 3M21 12l-3-3M21 12l-3 3" />,
    ),
  },
  {
    command: "COPY",
    label: "Copy",
    icon: icon(
      <>
        <rect x="4" y="4" width="11" height="11" />
        <rect x="9" y="9" width="11" height="11" />
      </>,
    ),
  },
  {
    command: "ERASE",
    label: "Erase",
    icon: icon(<path d="M5 19h14M7 15 15 5l4 3-8 10H8z" strokeLinejoin="round" />),
  },
];

function ToolGroup({ app, name, tools }: { app: CadApp; name: string; tools: readonly Tool[] }) {
  return (
    <div className="toolbar__group" role="group" aria-label={name}>
      {tools.map((tool) => (
        <button
          key={tool.command}
          type="button"
          className="toolbar__button"
          title={tool.command}
          // Keep keyboard focus on the command line.
          onMouseDown={(event) => event.preventDefault()}
          // No command logic here: the button names a command and Rust runs it
          // through the same entry point as typed input (DESIGN §5, §15).
          onClick={() => app.invoke(tool.command)}
        >
          {tool.icon}
          <span>{tool.label}</span>
        </button>
      ))}
    </div>
  );
}

export function Toolbar({ app }: { app: CadApp }) {
  return (
    <nav className="toolbar" aria-label="Tools">
      <ToolGroup app={app} name="Draw" tools={DRAW} />
      <ToolGroup app={app} name="Modify" tools={MODIFY} />
    </nav>
  );
}
