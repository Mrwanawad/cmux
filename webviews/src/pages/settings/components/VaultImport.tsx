// Settings > Cloud > Vault > Import from .env…: the app's native panel picks the file, and the
// card shows what the bundled `cmux vault import` read from it, names only: a checkbox per name,
// all ticked; a name the vault refuses (one cmux or the shell owns, `PATH`, `CMUX_…`) drawn off and
// unticked; one the vault already keeps in the chosen scope marked as replaced. The scope picker
// reads the file again for the new scope. Keep sends the ticked names and the file's token; the
// values go from the file to the vault inside Rust and never reach this page.
import { useEffect, useState } from "react";
import { Button } from "../../../ui/Button";
import { useStore } from "../context";
import type { VaultPreview } from "../ops";
import { t } from "../strings";
import { scopeWire, type ScopeChoice } from "../vault";
import { ScopePicker, scopeLabel } from "./VaultScope";

export function VaultImport({ onDone }: { onDone: (kept?: string) => Promise<void> }) {
  const store = useStore();
  const [scope, setScope] = useState<ScopeChoice>({ kind: "all", target: "" });
  const [preview, setPreview] = useState<VaultPreview | null>(null);
  const [ticked, setTicked] = useState<ReadonlySet<string>>(() => new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const wire = scopeWire(scope);
  const token = preview?.token;
  const pick = async () => {
    setBusy(true);
    setError(null);
    const answer = await store.vaultImportPreview(wire ?? "*");
    setBusy(false);
    if (answer.error) return setError(answer.error);
    if (!answer.preview || answer.preview.cancelled) return;
    setPreview(answer.preview);
    setTicked(new Set((answer.preview.preview ?? []).filter((row) => row.keepable).map((row) => row.name)));
  };
  // Another scope: the file is read again, so "already kept" names that scope's items.
  useEffect(() => {
    if (!token || wire === null) return;
    let live = true;
    void store.vaultImportPreview(wire, token).then((answer) => {
      if (!live) return;
      if (answer.error) setError(answer.error);
      else if (answer.preview) setPreview(answer.preview);
    });
    return () => {
      live = false;
    };
  }, [store, token, wire]);
  const keep = async () => {
    if (!token || wire === null || ticked.size === 0) return;
    setBusy(true);
    setError(null);
    const names = (preview?.preview ?? []).filter((row) => row.keepable && ticked.has(row.name)).map((row) => row.name);
    const answer = await store.vaultImportKeep(token, names, wire);
    setBusy(false);
    if (answer.error) return setError(answer.error);
    await onDone(t("settingsPage.vault.importKept", scopeLabel(wire)));
  };
  const rows = preview?.preview ?? [];
  const toggle = (name: string, on: boolean) =>
    setTicked((current) => {
      const next = new Set(current);
      if (on) next.add(name);
      else next.delete(name);
      return next;
    });
  return (
    <div className="host-form vault-import" data-vault-import={preview ? "picked" : "pick"}>
      <div className="row-title">{t("settingsPage.vault.importTitle")}</div>
      {!preview ? (
        <>
          <div className="row-help">{t("settingsPage.vault.importHelp")}</div>
          <div className="host-actions">
            <Button
              type="button"
              variant="solid"
              className="button"
              data-vault-action="choose-file"
              disabled={busy}
              onClick={() => void pick()}
            >
              {t("settingsPage.vault.chooseFile")}
            </Button>
            <Button type="button" className="button" data-vault-action="cancel" onClick={() => void onDone()}>
              {t("settingsPage.cancel")}
            </Button>
          </div>
        </>
      ) : (
        <>
          <div className="row-help selectable" data-vault-import-file="">
            {t("settingsPage.vault.importFrom", preview.file ?? "")}
          </div>
          {rows.length === 0 ? (
            <div className="row-help" data-vault-import-empty="">
              {t("settingsPage.vault.importNone")}
            </div>
          ) : (
            <ul className="vault-import-names" data-vault-import-names="">
              {rows.map((row) => (
                <li key={row.name} data-vault-import-name={row.name} data-keepable={row.keepable ? "" : undefined}>
                  <label className="vault-import-check">
                    <input
                      type="checkbox"
                      checked={row.keepable && ticked.has(row.name)}
                      disabled={!row.keepable || busy}
                      onChange={(event) => toggle(row.name, event.currentTarget.checked)}
                    />
                    <code className="keys-name">{row.name}</code>
                    <span className="row-help">
                      {row.masked}
                      {!row.keepable
                        ? ` · ${t("settingsPage.vault.importRefused")}`
                        : row.alreadyKept
                          ? ` · ${t("settingsPage.vault.importReplaces")}`
                          : ""}
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          )}
          {(preview.skippedLines?.length ?? 0) > 0 && (
            <div className="row-help" data-vault-import-skipped="">
              {t("settingsPage.vault.importSkipped", (preview.skippedLines ?? []).join(", "))}
            </div>
          )}
          <ScopePicker scope={scope} onChange={setScope} />
          <div className="host-actions">
            <Button
              type="button"
              variant="solid"
              className="button"
              data-vault-action="keep"
              disabled={busy || wire === null || ticked.size === 0}
              onClick={() => void keep()}
            >
              {t("settingsPage.vault.keep")}
            </Button>
            <Button type="button" className="button" data-vault-action="cancel" onClick={() => void onDone()}>
              {t("settingsPage.cancel")}
            </Button>
          </div>
        </>
      )}
      {error && (
        <div className="row-error" role="alert" data-vault-import-error="">
          {error}
        </div>
      )}
    </div>
  );
}
