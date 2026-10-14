import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import globals from 'globals';
import hooks from 'eslint-plugin-react-hooks';
import refresh from 'eslint-plugin-react-refresh';
import prettier from 'eslint-config-prettier';
import { plugin as shadcn } from '@shadcn/lint';

export default tseslint.config(
  {
    ignores: [
      '**/node_modules/**',
      '**/dist/**',
      '.data/**',
      '.run/**',
      'tmp/**',
      '.cache/**',
      'vendor/**',
    ],
  },
  {
    files: [
      'src/**/*.ts',
      'packages/contracts/src/**/*.ts',
      'scripts/**/*.{ts,js,mjs}',
      'tests/**/*.{ts,js,mjs}',
      'web/**/*.{ts,tsx}',
      'browser-extension/**/*.js',
      '*.js',
    ],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: { globals: globals.node },
    rules: {
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrorsIgnorePattern: '^_' },
      ],
    },
  },
  {
    files: ['scripts/**/*.cjs'],
    extends: [js.configs.recommended],
    languageOptions: { globals: globals.node, sourceType: 'commonjs' },
  },
  {
    files: ['web/src/**/*.{ts,tsx}'],
    languageOptions: { globals: globals.browser },
    plugins: { 'react-hooks': hooks, 'react-refresh': refresh, shadcn },
    settings: { shadcn: { componentImports: ['^@/components/reui(/|$)'] } },
    rules: {
      'shadcn/no-restyle': ['error', { allow: ['layout'] }],
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'error',
      'react-refresh/only-export-components': ['error', { allowConstantExport: true }],
    },
  },
  {
    files: ['browser-extension/**/*.js'],
    languageOptions: { globals: { ...globals.browser, chrome: 'readonly' } },
  },
  {
    files: ['packages/contracts/src/**/*.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['node:*', '@teable/*', 'onecrew/*', '../../*'],
              message: 'Shared contracts must stay browser-safe and independent of the server.',
            },
          ],
        },
      ],
    },
  },
  {
    // Registry primitives intentionally export variants, contexts and hooks together.
    files: ['web/src/components/{ui,reui}/**/*.tsx'],
    rules: { 'react-refresh/only-export-components': 'off', 'shadcn/no-restyle': 'off' },
  },
  prettier,
);
