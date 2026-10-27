import react from "@vitejs/plugin-react";
import { loadEnv, type Plugin } from "vite";
import { defineConfig } from "vitest/config";

/**
 * The page's content-security policy, in the built page only (the dev
 * server's hot reload runs inline scripts): scripts and WASM from the page's
 * own origin, connections to it and to the AI service the build names, and
 * nothing else fetched from anywhere (SECURITY.md).
 */
function contentSecurityPolicy(service: string | undefined): Plugin {
  const connect = ["'self'", ...(service ? [new URL(service).origin] : [])].join(" ");
  const policy = [
    "default-src 'self'",
    "script-src 'self' 'wasm-unsafe-eval'",
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data: blob:",
    "font-src 'self'",
    `connect-src ${connect}`,
    "worker-src 'self' blob:",
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
  ].join("; ");
  return {
    name: "content-security-policy",
    apply: "build",
    transformIndexHtml: () => [
      {
        tag: "meta",
        attrs: { "http-equiv": "Content-Security-Policy", content: policy },
        injectTo: "head-prepend",
      },
    ],
  };
}

export default defineConfig(({ mode }) => ({
  // Relative base so the static bundle works from any path (ADR-0004).
  base: "./",
  plugins: [
    react(),
    contentSecurityPolicy(loadEnv(mode, process.cwd(), "VITE_").VITE_AI_SERVICE_URL),
  ],
  build: { target: "es2022" },
  test: { include: ["src/**/*.test.ts", "src/**/*.test.tsx"] },
}));
