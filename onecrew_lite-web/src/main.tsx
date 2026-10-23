import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClientProvider } from '@tanstack/react-query'
import { createLocalQueryClient } from './lib/local-query-client'
import { I18nextProvider } from 'react-i18next'
import './styles/index.css'
import App from './App'
import { AppErrorBoundary } from './components/app-error-boundary'
import { appI18n, initializeI18n } from './i18n'
import { connectLocalRuntime } from './lib/local-runtime'
import { syncPresentationLocale } from './i18n/presentation-locale'
import { resolveInitialLocale } from './i18n/locale'

const queryClient = createLocalQueryClient()

await initializeI18n()
// A failed handshake renders App's connection gate instead of pretending that
// browser execution is a native application or enabling business queries.
await connectLocalRuntime().catch(() => undefined)
await syncPresentationLocale(resolveInitialLocale({ storedLocale: appI18n.language })).catch(() => undefined)

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <I18nextProvider i18n={appI18n}>
      <AppErrorBoundary>
        <QueryClientProvider client={queryClient}>
          <App />
        </QueryClientProvider>
      </AppErrorBoundary>
    </I18nextProvider>
  </StrictMode>,
)
