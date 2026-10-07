import tseslint from '@typescript-eslint/eslint-plugin';
import tsparser from '@typescript-eslint/parser';
import solid from 'eslint-plugin-solid/configs/typescript';

export default [
  {
    files: ['src/**/*.{ts,tsx}'],
    languageOptions: {
      parser: tsparser,
      parserOptions: { project: './tsconfig.json' },
    },
    plugins: {
      '@typescript-eslint': tseslint,
      solid: solid.plugins?.solid,
    },
    rules: {
      ...tseslint.configs.recommended.rules,
      ...(solid.rules || {}),
    },
  },
];
