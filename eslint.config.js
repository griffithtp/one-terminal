import tseslint from "typescript-eslint";
import reactPlugin from "eslint-plugin-react";
import reactHooksPlugin from "eslint-plugin-react-hooks";
import jsxA11y from "eslint-plugin-jsx-a11y";
import prettierConfig from "eslint-config-prettier";

// The three UIs; type-aware rules and accessibility rules apply here.
const UI_SOURCES = [
  "apps/one-terminal/src/**/*.{ts,tsx}",
  "apps/desktop-agent/src/**/*.{ts,tsx}",
  "apps/app-directory/ui/**/*.{ts,tsx}",
];

export default tseslint.config(
  {
    ignores: [
      "**/dist/**",
      "**/node_modules/**",
      "**/target/**",
      "packages/create-one-terminal/templates/**",
      "packages/fdc3-plugin/**",
      "apps/sample-ticker/**",
      "apps/sample-chart/**",
      "apps/electron-host/**",
    ],
  },
  {
    files: ["**/*.ts", "**/*.tsx"],
    extends: [...tseslint.configs.recommended],
    rules: {
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      // docs/conventions/general.md → TypeScript
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/consistent-type-imports": "error",
      "@typescript-eslint/no-non-null-assertion": "error",
    },
  },
  {
    // Rules that need type information (tsconfig per app).
    files: UI_SOURCES,
    languageOptions: {
      parserOptions: { projectService: true, tsconfigRootDir: import.meta.dirname },
    },
    rules: {
      // Every promise is awaited, returned, .catch-ed, or explicitly `void`-ed.
      "@typescript-eslint/no-floating-promises": "error",
      // Unions are handled case by case, not swept into `default`.
      "@typescript-eslint/switch-exhaustiveness-check": "error",
    },
  },
  {
    files: [
      "apps/one-terminal/src/**/*.tsx",
      "apps/desktop-agent/src/**/*.tsx",
      "apps/app-directory/ui/**/*.tsx",
    ],
    plugins: {
      react: reactPlugin,
      "react-hooks": reactHooksPlugin,
      "jsx-a11y": jsxA11y,
    },
    settings: {
      react: { version: "detect" },
    },
    rules: {
      ...reactPlugin.configs.recommended.rules,
      ...reactHooksPlugin.configs.recommended.rules,
      ...jsxA11y.flatConfigs.recommended.rules,
      "react/react-in-jsx-scope": "off",
      "react/prop-types": "off",
      // react-hooks v5 React Compiler rules — fire on valid patterns in non-Compiler codebases
      "react-hooks/refs": "off",
      "react-hooks/set-state-in-effect": "off",
      "react-hooks/static-components": "off",
      // Dialogs, the command palette and rename fields move focus into themselves when
      // they open — the WAI-ARIA dialog pattern. The rule targets focus theft on page load.
      "jsx-a11y/no-autofocus": "off",
    },
  },
  prettierConfig
);
