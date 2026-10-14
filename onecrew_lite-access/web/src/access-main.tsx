import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router";
import { I18nextProvider } from "react-i18next";

import { AccessRoot } from "@/access-app";
import accessI18n from "@/lib/i18n";
import "@/styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <I18nextProvider i18n={accessI18n}>
      <BrowserRouter>
        <AccessRoot />
      </BrowserRouter>
    </I18nextProvider>
  </StrictMode>,
);
