import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint from "typescript-eslint";

// Architecture rules from AGENTS.md, enforced as lint errors:
//  - only src/cad/ may import the generated WASM package;
//  - only src/renderer/ may touch WebGL.
const wasmPackageOnlyInCad = {
  patterns: [
    {
      group: ["**/cad/pkg", "**/cad/pkg/**", "**/pkg/cad_wasm*"],
      message: "Import the WASM package only from src/cad/ (ADR-0002).",
    },
  ],
};

const webglOnlyInRenderer = [
  {
    selector: "CallExpression[callee.property.name='getContext']",
    message: "Create graphics contexts only in src/renderer/ (DESIGN §12.1).",
  },
  {
    selector: "TSTypeReference[typeName.name=/^WebGL/]",
    message: "WebGL types belong in src/renderer/ (DESIGN §12.1).",
  },
];

export default tseslint.config(
  { ignores: ["dist", "src/cad/pkg", "playwright-report", "test-results"] },
  js.configs.recommended,
  ...tseslint.configs.strict,
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: { globals: globals.browser },
    plugins: { "react-hooks": reactHooks },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "no-restricted-imports": ["error", wasmPackageOnlyInCad],
      "no-restricted-syntax": ["error", ...webglOnlyInRenderer],
    },
  },
  { files: ["src/cad/**"], rules: { "no-restricted-imports": "off" } },
  // Renderer code owns WebGL; end-to-end tests may poke the context to simulate failures.
  { files: ["src/renderer/**", "e2e/**"], rules: { "no-restricted-syntax": "off" } },
  { files: ["*.config.{js,ts}", "e2e/**"], languageOptions: { globals: globals.node } },
);
