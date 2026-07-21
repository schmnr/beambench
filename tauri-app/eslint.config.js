import { readFileSync } from 'node:fs';
import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';
import i18next from 'eslint-plugin-i18next';

const allowlistObj = JSON.parse(
  readFileSync(new URL('./.i18n-lint-exceptions.json', import.meta.url), 'utf-8'),
);
const allowlistFiles = Object.keys(allowlistObj).filter((k) => !k.startsWith('_'));

export default tseslint.config(
  { ignores: ['dist'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    plugins: { 'react-hooks': reactHooks },
    rules: {
      ...reactHooks.configs.recommended.rules,
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
    },
  },
  // i18next rule active for component files. Test files and non-component
  // sources are excluded — we only translate user-facing strings.
  {
    files: ['src/components/**/*.tsx', 'src/panels/**/*.tsx'],
    ignores: [
      'src/**/*.test.{ts,tsx}',
      'src/**/__tests__/**/*.{ts,tsx}',
      ...allowlistFiles,
    ],
    plugins: { i18next },
    rules: {
      'i18next/no-literal-string': [
        'error',
        {
          mode: 'jsx-only',
          'jsx-attributes': {
            include: ['title', 'placeholder', 'aria-label', 'alt'],
            exclude: [
              'className', 'selectClassName', 'containerClassName', 'zIndexClassName', 'backdropClassName',
              'shortcut',
              'data-testid', 'testId', 'role', 'href', 'src', 'id',
              'titleId', 'toolKind', 'seedOperation', 'previewState', 'name', 'type', 'key', 'rel', 'target', 'autoComplete',
              'value', 'defaultValue', 'min', 'max', 'step', 'pattern',
              'method', 'action', 'autoCapitalize', 'autoCorrect', 'spellCheck',
              'inputMode', 'enterKeyHint', 'capture', 'accept', 'crossOrigin',
              'aria-controls', 'aria-describedby', 'aria-labelledby', 'aria-haspopup',
              'data-active-tab', 'data-mode', 'data-state',
              'direction', 'zone',
            ],
          },
          'jsx-components': {
            exclude: ['Trans', 'code', 'pre', 'script', 'style'],
          },
          callees: {
            exclude: [
              't',
              'i18n.t',
              'i18next.t',
              'ml',
              'executeAppCommand',
              'handleAlign',
              'handleDistribute',
              'handleMoveTogether',
              'handlePrint',
              'pushDrawOrder',
              'deleteDuplicateObjects',
              'openFeedbackReport',
              'checkForUpdates',
              'changeLanguage',
              'pushNotification',
              'updateDraft',
              'updateSettings',
              'update',
              'setActiveTab',
              'setOpenMenu',
              'resolveConflicts',
              'setSaveError',
              'setConflictFields',
              'setSettings',
              'updateField',
              'handleUpdate',
              'renderAxisEditor',
              'setStatus',
              'setMode',
              'setDistanceThreshold',
              'setCount',
              'setRotateCopies',
              'setScaleCopies',
              'setFinalScalePercent',
              'setBusy',
              'setDirection',
              'setCornerStyle',
              'setDeleteOriginal',
              'setDistance',
              'setNotes',
              'require',
              'import',
              'invoke',
              'emit',
              'listen',
              'addEventListener',
              'removeEventListener',
              'querySelector',
              'querySelectorAll',
              'getAttribute',
              'setAttribute',
              'dispatchEvent',
              'console.log',
              'console.warn',
              'console.error',
            ],
          },
          'should-validate-template': false,
        },
      ],
    },
  },
  {
    files: ['src/**/*.test.{ts,tsx}', 'src/**/__tests__/**/*.{ts,tsx}'],
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
    },
  },
);
