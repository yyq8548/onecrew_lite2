import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { I18nextProvider } from 'react-i18next';
import { describe, expect, it } from 'vitest';

import { appI18n } from '@/i18n';
import { resources } from '@/i18n/resources';
import { RefreshButton } from './refresh-button';

describe('RefreshButton', () => {
  it('supports an outlined icon treatment without losing its accessible label', async () => {
    if (!appI18n.isInitialized) {
      await appI18n.init({ fallbackLng: 'zh-CN', lng: 'zh-CN', resources });
    }
    const markup = renderToStaticMarkup(
      createElement(
        I18nextProvider,
        { i18n: appI18n },
        createElement(RefreshButton, {
          label: '刷新会话列表',
          variant: 'outline',
          onClick: () => undefined,
        }),
      ),
    );

    expect(markup).toContain('border bg-background');
    expect(markup).toContain('aria-label="刷新会话列表"');
    expect(markup).toContain('title="刷新会话列表"');
  });
});
