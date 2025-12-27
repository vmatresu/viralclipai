// @ts-check

import eslint from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  // Global ignores
  {
    ignores: ["dist/**", "node_modules/**", "**/*.mjs", "eslint.config.js"],
  },
  // Base ESLint recommended rules
  eslint.configs.recommended,
  // TypeScript-ESLint recommended rules
  ...tseslint.configs.recommended,
  // Custom configuration for TypeScript files
  {
    files: ["**/*.ts"],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // Allow unused vars with underscore prefix
      "@typescript-eslint/no-unused-vars": [
        "error",
        {
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
        },
      ],
      // Allow explicit any for now (gradual strictness)
      "@typescript-eslint/no-explicit-any": "warn",
      // Allow empty catch blocks with comment
      "no-empty": ["error", { allowEmptyCatch: true }],
    },
  }
);
