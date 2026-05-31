import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import jsxA11y from "eslint-plugin-jsx-a11y";
import globals from "globals";
import prettier from "eslint-config-prettier";

export default tseslint.config(
  {
    // Build output, vendored configs, and the Rust/docs trees are out of
    // scope — the frontend lint surface is `src/`.
    ignores: [
      "dist",
      "coverage",
      "node_modules",
      "src-tauri",
      "docs",
      "scripts",
      "*.config.{js,ts}",
      "src/vite-env.d.ts",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
    plugins: {
      "react-hooks": reactHooks,
      "jsx-a11y": jsxA11y,
    },
    rules: {
      // Long-standing hook-correctness core. We deliberately do NOT pull
      // in react-hooks 7's full `recommended-latest`, which bundles the
      // React Compiler rules (set-state-in-effect, refs-during-render).
      // Those target Compiler adoption; this app doesn't use the Compiler
      // and its draft-sync / stable-key-ref patterns are intentional.
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
      ...jsxA11y.flatConfigs.recommended.rules,
      // Underscore-prefixed bindings are intentionally unused (ignored
      // callback args, placeholder destructures).
      "@typescript-eslint/no-unused-vars": [
        "error",
        {
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
        },
      ],
    },
  },
  // Disable every stylistic rule that would conflict with Prettier; must
  // stay last so it wins.
  prettier,
);
