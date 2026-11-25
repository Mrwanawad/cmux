// Settings > Cloud > Vault (cmuxterm-hq docs/design/e2e-vault.md). Leo, 2026-11-25: "if we're like
// Dropbox/Google Drive, I suppose it's no big deal to ask people and have them opt in to sync
// whatever/upload whatever/add whatever stuff if they want. And we just do it securely and safely".
// Opt-in: off until Turn On (cloud.vault.enabled), which also adds this Mac (the first device makes
// the vault; a later one shows its 8-digit code and waits for an approve on another device). On:
// the devices with Approve and Remove, the items masked with Add, Edit and Remove, and Import from
// .env with a checkbox per name. A value is typed into a password field, sent once to the app
// (which pipes it to `cmux vault set` on stdin) and the field empties; nothing here ever receives a
// value back. The rules are Rust's (cmux-tui cli/vault.rs, crate cmux-vault-e2e).
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useLayoutEffect, useRef, useState, type FormEvent } from "react";
import { Button } from "../../../ui/Button";
import { Input } from "../../../ui/Input";
import { useSettingsSelector, useStore } from "../context";
import type { VaultDeviceWire, VaultItemWire, VaultRun } from "../ops";
import { VAULT_KEY, vaultQuery } from "../sectionQueries";
import { valueOf } from "../store";
import { t } from "../strings";
import {
  VAULT_SETTING,
  codeText,
  deviceRows,
  itemRows,
  plausibleName,
  scopeChoice,
  scopeWire,
  vaultPhase,
  type ScopeChoice,
} from "../vault";
import { VaultImport } from "./VaultImport";
import { ScopePicker, scopeLabel } from "./VaultScope";

type Run = (run: VaultRun) => Promise<string | null>;

export function VaultCard({ reveal = false }: { reveal?: boolean }) {
  const store = useStore();
  const queryClient = useQueryClient();
  const enabled = useSettingsSelector((state) => valueOf(state, VAULT_SETTING) === true);
  const query = useQuery({ ...vaultQuery(store), enabled });
  const read = query.data ?? null;
  const phase = vaultPhase(enabled, read);
  const card = useRef<HTMLElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useLayoutEffect(() => {
    if (reveal) card.current?.scrollIntoView?.({ block: "start" });
  }, [reveal]);
  const refresh = () => queryClient.invalidateQueries({ queryKey: VAULT_KEY });
  // The store answers a refusal as text and never throws, so busy always clears.
  const run: Run = async (gesture) => {
    setBusy(true);
    setError(null);
    const refused = await store.runVault(gesture);
    setError(refused);
    await refresh();
    setBusy(false);
    return refused;
  };
  const turnOn = async () => {
    setBusy(true);
    setError(null);
    const wrote = await store.set(VAULT_SETTING, true);
    const refused = wrote.ok ? await store.runVault({ action: "on" }) : null;
    setError(refused);
    if (wrote.ok) await refresh();
    setBusy(false);
  };
  const turnOff = () => void store.set(VAULT_SETTING, false);
  const status = read?.status ?? null;
  return (
    <section className="group" data-card="vault" data-vault-phase={phase} id="vault" ref={card}>
      <h3 className="group-title">{t("settingsPage.vault.title")}</h3>
      <p className="row-help">{t("settingsPage.vault.lead")}</p>
      {phase !== "on" && (
        <div className="rows" data-vault-state="">
          {phase === "off" && (
            <div className="row" data-vault-off="">
              <span className="row-help">{t("settingsPage.vault.offHelp")}</span>
              <span className="row-control">
                <Button
                  type="button"
                  variant="solid"
                  className="button"
                  data-vault-action="on"
                  disabled={busy}
                  onClick={() => void turnOn()}
                >
                  {t("settingsPage.vault.turnOn")}
                </Button>
              </span>
            </div>
          )}
          {phase === "loading" && (
            <div className="row" data-vault-loading="">
              <span className="row-help">{t("settingsPage.vault.loading")}</span>
            </div>
          )}
          {(phase === "signedOut" || phase === "error") && (
            <div className="row" data-vault-unavailable="">
              <span className="row-help">
                {phase === "signedOut"
                  ? t("settingsPage.vault.signedOut")
                  : (read?.error ?? t("settingsPage.vault.failed"))}
              </span>
              <span className="row-control">
                <Button
                  type="button"
                  className="button"
                  data-vault-action="retry"
                  disabled={busy}
                  onClick={() => void refresh()}
                >
                  {t("settingsPage.vault.retry")}
                </Button>
              </span>
            </div>
          )}
          {phase === "joining" && (
            <div className="row" data-vault-joining="">
              <span className="row-help">
                {status?.state === "not_enrolled"
                  ? t("settingsPage.vault.joinOthers")
                  : t("settingsPage.vault.joining")}
              </span>
              <span className="row-control">
                <Button
                  type="button"
                  variant="solid"
                  className="button"
                  data-vault-action="join"
                  disabled={busy}
                  onClick={() => void run({ action: "on" })}
                >
                  {t("settingsPage.vault.addThisMac")}
                </Button>
              </span>
            </div>
          )}
          {phase === "pending" && (
            <div className="row vault-pending" data-vault-pending="">
              <span className="row-help">{t("settingsPage.vault.pendingHelp")}</span>
              <code className="vault-code" data-vault-code="" aria-label={t("settingsPage.vault.codeLabel")}>
                {codeText(status?.safetyCode)}
              </code>
              <span className="row-help">
                <code className="keys-code">cmux vault approve {status?.safetyCode ?? ""}</code>
              </span>
            </div>
          )}
        </div>
      )}
      {phase === "on" && status && (
        <>
          <h4 className="vault-subtitle">{t("settingsPage.vault.devices")}</h4>
          <div className="rows" data-vault-devices="">
            {deviceRows(read).map((device) => (
              <DeviceRow key={device.id} device={device} busy={busy} run={run} />
            ))}
          </div>
          <h4 className="vault-subtitle">{t("settingsPage.vault.items")}</h4>
          <div className="rows" data-vault-items="">
            {itemRows(read).length === 0 ? (
              <div className="row" data-vault-empty="">
                <span className="row-help">{t("settingsPage.vault.empty")}</span>
              </div>
            ) : (
              itemRows(read).map((item) => <ItemRow key={item.id} item={item} busy={busy} run={run} />)
            )}
          </div>
          <VaultActions busy={busy} run={run} onImported={refresh} />
        </>
      )}
      {(phase === "pending" || phase === "on" || phase === "joining") && (
        <div className="host-footer">
          <Button
            type="button"
            variant="ghost"
            className="button"
            data-vault-action="off"
            disabled={busy}
            onClick={turnOff}
          >
            {t("settingsPage.vault.turnOff")}
          </Button>
          <span className="row-help">{t("settingsPage.vault.turnOffHelp")}</span>
        </div>
      )}
      {error && (
        <div className="row-error" role="alert" data-vault-error="">
          {error}
        </div>
      )}
    </section>
  );
}

const KINDS: Record<string, string> = {
  mac: "settingsPage.vault.kind.mac",
  iphone: "settingsPage.vault.kind.iphone",
  cli: "settingsPage.vault.kind.cli",
  linux: "settingsPage.vault.kind.linux",
};

function DeviceRow({ device, busy, run }: { device: VaultDeviceWire; busy: boolean; run: Run }) {
  const [asking, setAsking] = useState<"approve" | "remove" | null>(null);
  const pending = device.state === "pending";
  const kind = KINDS[device.kind] ? t(KINDS[device.kind]!) : device.kind;
  const state = pending ? t("settingsPage.vault.statePending") : t("settingsPage.vault.stateActive");
  const label = device.thisDevice ? t("settingsPage.vault.thisMac", device.label) : device.label;
  return (
    <div className="row" data-vault-device={device.id} data-vault-device-state={device.state}>
      <div className="row-main">
        <div className="row-label">
          <div className="row-title">{label}</div>
          <div className="row-help selectable" data-vault-device-detail="">
            {kind} · {t("settingsPage.vault.code", codeText(device.safetyCode))}
          </div>
        </div>
        <span className="row-control keys-control">
          <span className="accounts-status devices-status" data-tone={pending ? "attention" : "success"}>
            <span className="accounts-dot" aria-hidden="true" />
            {state}
          </span>
          {pending && asking === null && (
            <Button
              type="button"
              className="button"
              data-vault-action="approve"
              disabled={busy}
              onClick={() => setAsking("approve")}
            >
              {t("settingsPage.vault.approve")}
            </Button>
          )}
          {!device.thisDevice && asking === null && (
            <Button
              danger
              variant="ghost"
              type="button"
              className="button"
              data-vault-action="remove-device"
              disabled={busy}
              onClick={() => setAsking("remove")}
            >
              {t("settingsPage.remove")}
            </Button>
          )}
        </span>
      </div>
      {asking && (
        // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role
        <div className="accounts-form" data-vault-confirm={asking} role="group">
          <div className="row-title">
            {asking === "approve"
              ? t("settingsPage.vault.approveTitle", device.label)
              : t("settingsPage.vault.removeDeviceTitle", device.label)}
          </div>
          {asking === "approve" && (
            <code className="vault-code" data-vault-confirm-code="">
              {codeText(device.safetyCode)}
            </code>
          )}
          <div className="row-help">
            {asking === "approve" ? t("settingsPage.vault.approveBody") : t("settingsPage.vault.removeDeviceBody")}
          </div>
          <div className="accounts-buttons">
            <Button
              type="button"
              variant={asking === "approve" ? "solid" : "ghost"}
              danger={asking === "remove"}
              className="button"
              data-vault-action={asking === "approve" ? "confirm-approve" : "confirm-remove-device"}
              disabled={busy}
              onClick={() =>
                void run(
                  asking === "approve"
                    ? { action: "approve", code: device.safetyCode }
                    : { action: "remove_device", id: device.id },
                ).then(() => setAsking(null))
              }
            >
              {asking === "approve" ? t("settingsPage.vault.codesMatch") : t("settingsPage.remove")}
            </Button>
            <Button type="button" className="button" data-vault-action="cancel" onClick={() => setAsking(null)}>
              {t("settingsPage.cancel")}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

function ItemRow({ item, busy, run }: { item: VaultItemWire; busy: boolean; run: Run }) {
  const [mode, setMode] = useState<"edit" | "remove" | null>(null);
  return (
    <div className="row" data-vault-item={item.name} data-vault-scope={item.scope}>
      <div className="row-main">
        <div className="row-label">
          <div className="row-title">
            <code className="keys-name">{item.name}</code>
          </div>
          <div className="row-help" data-vault-item-detail="">
            <span className="vault-masked" aria-label={t("settingsPage.vault.hidden")}>
              {item.masked}
            </span>{" "}
            · {scopeLabel(item.scope)}
          </div>
        </div>
        {mode === null && (
          <span className="row-control keys-control">
            <Button
              type="button"
              className="button"
              data-vault-action="edit"
              disabled={busy}
              onClick={() => setMode("edit")}
            >
              {t("settingsPage.vault.edit")}
            </Button>
            <Button
              danger
              variant="ghost"
              type="button"
              className="button"
              data-vault-action="remove"
              disabled={busy}
              onClick={() => setMode("remove")}
            >
              {t("settingsPage.remove")}
            </Button>
          </span>
        )}
      </div>
      {mode === "edit" && <ItemForm item={item} busy={busy} run={run} onDone={() => setMode(null)} />}
      {mode === "remove" && (
        // oxlint-disable-next-line jsx-a11y/prefer-tag-over-role
        <div className="accounts-form" data-vault-confirm="remove" role="group">
          <div className="row-title">{t("settingsPage.vault.removeTitle", item.name)}</div>
          <div className="row-help">{t("settingsPage.vault.removeBody")}</div>
          <div className="accounts-buttons">
            <Button
              danger
              variant="ghost"
              type="button"
              className="button"
              data-vault-action="confirm-remove"
              disabled={busy}
              onClick={() =>
                void run({ action: "remove", name: item.name, scope: item.scope }).then(() => setMode(null))
              }
            >
              {t("settingsPage.remove")}
            </Button>
            <Button type="button" className="button" data-vault-action="cancel" onClick={() => setMode(null)}>
              {t("settingsPage.cancel")}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

function VaultActions({ busy, run, onImported }: { busy: boolean; run: Run; onImported: () => Promise<void> }) {
  const [open, setOpen] = useState<"add" | "import" | null>(null);
  const [said, setSaid] = useState<string | null>(null);
  return (
    <>
      {open === "add" && <ItemForm busy={busy} run={run} onDone={() => setOpen(null)} />}
      {open === "import" && (
        <VaultImport
          onDone={async (message) => {
            setOpen(null);
            setSaid(message ?? null);
            if (message) await onImported();
          }}
        />
      )}
      {open === null && (
        <div className="host-footer">
          <Button
            type="button"
            className="button"
            data-vault-action="add"
            disabled={busy}
            onClick={() => {
              setSaid(null);
              setOpen("add");
            }}
          >
            {t("settingsPage.vault.add")}
          </Button>
          <Button
            type="button"
            className="button"
            data-vault-action="import"
            disabled={busy}
            onClick={() => {
              setSaid(null);
              setOpen("import");
            }}
          >
            {t("settingsPage.vault.import")}
          </Button>
          {said && (
            <span className="row-help" role="status" data-vault-import-kept="">
              {said}
            </span>
          )}
        </div>
      )}
    </>
  );
}

/** Add, or Edit with `item`: the name (fixed on Edit), the value in a password field, the scope.
 * The value lives in this form until Save sends it, then the field empties; the old value is never
 * shown, so Edit takes a new one. */
function ItemForm({ item, busy, run, onDone }: { item?: VaultItemWire; busy: boolean; run: Run; onDone: () => void }) {
  const [name, setName] = useState(item?.name ?? "");
  const [value, setValue] = useState("");
  const [scope, setScope] = useState<ScopeChoice>(scopeChoice(item?.scope));
  const wire = scopeWire(scope);
  const ready = plausibleName(name.trim()) && value !== "" && wire !== null && !busy;
  const save = (event: FormEvent) => {
    event.preventDefault();
    if (!ready || wire === null) return;
    const sent = value;
    setValue("");
    void run({
      action: "set",
      name: name.trim(),
      scope: wire,
      value: sent,
      ...(item && item.scope !== wire ? { replace_scope: item.scope } : {}),
    }).then((refused) => {
      if (refused === null) onDone();
    });
  };
  return (
    <form className="host-form" data-vault-form={item?.name ?? "new"} onSubmit={save}>
      <label className="host-field">
        <span className="row-help">{t("settingsPage.vault.name")}</span>
        <Input
          className="field font-mono"
          data-vault-field="name"
          aria-label={t("settingsPage.vault.name")}
          placeholder="OPENAI_API_KEY"
          spellCheck={false}
          autoComplete="off"
          readOnly={item !== undefined}
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
      </label>
      <label className="host-field">
        <span className="row-help">{item ? t("settingsPage.vault.newValue") : t("settingsPage.vault.value")}</span>
        <Input
          className="field keys-field"
          type="password"
          autoComplete="off"
          spellCheck={false}
          data-vault-field="value"
          aria-label={t("settingsPage.vault.value")}
          value={value}
          onChange={(event) => setValue(event.target.value)}
        />
      </label>
      <ScopePicker scope={scope} onChange={setScope} />
      <div className="host-actions">
        <Button type="submit" variant="solid" className="button" data-vault-action="save" disabled={!ready}>
          {t("settingsPage.vault.save")}
        </Button>
        <Button type="button" className="button" data-vault-action="cancel" onClick={onDone}>
          {t("settingsPage.cancel")}
        </Button>
      </div>
    </form>
  );
}
