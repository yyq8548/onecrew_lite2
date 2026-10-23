import React from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import './index.css';
import './i18n';
import { unifiedRuntimeMode } from './lib/unified-runtime';

if (unifiedRuntimeMode) {
  window.addEventListener('hashchange', () => {
    if (new URLSearchParams(window.location.hash.slice(1)).has('onecrew_ticket')) {
      window.location.reload();
    }
  });
}

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
