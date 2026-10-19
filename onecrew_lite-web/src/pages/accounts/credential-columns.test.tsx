import { renderToStaticMarkup } from "react-dom/server";
import type { ColumnDef } from "@tanstack/react-table";
import i18next from "i18next";
import { I18nextProvider } from "react-i18next";
import { describe, expect, it, vi } from "vitest";

import { DataTable } from "@/components/data-table";
import { resources } from "@/i18n/resources";
import type { ServiceCredential } from "@/types";

import ACCOUNTS_SOURCE from "../Accounts.tsx?raw";
import {
  credentialRows,
  youtubeCredentialColumns,
  type CredentialRow,
} from "./credential-columns";

const testI18n = i18next.createInstance();
// Mirror the app bootstrap: interpolated values (a formatted date) reach the
// DOM as text, not as HTML entities.
void testI18n.init({ initAsync: false, interpolation: { escapeValue: false }, lng: "zh-CN", resources });
const t = testI18n.getFixedT("zh-CN", "accounts");
const locale = "zh-CN";

function credential(overrides: Partial<ServiceCredential> = {}): ServiceCredential {
  return {
    id: "credential-1",
    provider: "youtube",
    label: "youtube-primary",
    metadata: {},
    disabled: false,
    cooldown_until: null,
    last_used_at: null,
    last_error: null,
    created_at: "1",
    updated_at: "1",
    ...overrides,
  };
}

function renderTable(
  columns: ColumnDef<CredentialRow>[],
  rows: CredentialRow[],
  emptyText: string,
) {
  return renderToStaticMarkup(
    <I18nextProvider i18n={testI18n}>
      <DataTable
        columns={columns}
        data={rows}
        emptyText={emptyText}
        pinnedRight={["actions"]}
        pagination={false}
        layout="grid"
        surface="embedded"
      />
    </I18nextProvider>,
  );
}

function youtubeTable(rows: CredentialRow[]) {
  return renderTable(
    youtubeCredentialColumns({
      t,
      locale,
      onTest: vi.fn(),
      onDelete: vi.fn(),
      onChanged: vi.fn(),
    }),
    rows,
    t("credentials.empty.youtube"),
  );
}


describe("credential tables", () => {
  it("renders every YouTube column and pins the row's controls to the right", () => {
    const lastUsedAt = 1_757_000_000;
    const html = youtubeTable(credentialRows(
      [credential({ metadata: { project: "discovery-project" }, last_used_at: lastUsedAt })],
      null,
    ));

    for (const column of ["credential", "projectLabel", "status", "recentStatus", "actions"] as const) {
      expect(html).toContain(t(`credentials.columns.${column}`));
    }
    expect(html).toContain(t("credentials.localOnlyMasked"));
    expect(html).toContain("discovery-project");
    expect(html).toContain(t("credentials.statuses.available"));
    expect(html).toContain(t("credentials.lastCalled", {
      date: new Date(lastUsedAt * 1000).toLocaleString(locale),
    }));
    for (const action of ["testApiKey", "delete", "edit"] as const) {
      expect(html).toContain(`aria-label="${t(`credentials.actions.${action}`)}"`);
    }
    // Header cell and body cell: the actions column is sticky and, being
    // pinned, keeps its full width while the rest of the table scrolls.
    expect(html.match(/position:sticky/g)).toHaveLength(2);
    expect(html.match(/flex:0 0 132px/g)).toHaveLength(2);
  });


  it("shows a failing credential's status and recent error, and the masked local-only line", () => {
    const html = youtubeTable(credentialRows(
      [credential({ last_error: "provider rejected the key" })],
      null,
    ));
    expect(html).toContain(t("credentials.statuses.needsAttention"));
    expect(html).toContain("provider rejected the key");
    expect(youtubeTable(credentialRows([credential({ disabled: true })], null)))
      .toContain(t("credentials.statuses.unavailable"));
  });

  it("disables the busy row's test control and every row's delete and edit", () => {
    const row = credential();
    const html = youtubeTable(credentialRows([row], row.id));
    expect(html).toContain("animate-spin");
    // test (this row is busy) + delete + edit.
    expect(html.match(/disabled=""/g)).toHaveLength(3);
    expect(youtubeTable(credentialRows([row], null)).match(/disabled=""/g)).toBeNull();
  });

  it("keeps the empty state of each credential list", () => {
    expect(youtubeTable([])).toContain(t("credentials.empty.youtube"));
  });

  it("keeps account management scoped to Gmail", () => {
    expect(ACCOUNTS_SOURCE).not.toContain("youtubeCredentialColumns");
    expect(ACCOUNTS_SOURCE).not.toContain("resendCredentialColumns");
    expect(ACCOUNTS_SOURCE).toContain("GmailConnection");
    expect(ACCOUNTS_SOURCE).not.toMatch(/grid-cols-\[/);
  });
});
