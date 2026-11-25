// l10n-allow-file: mock fixtures (sample device labels and names), not shipped UI.
// The mock host's Settings > Cloud > Vault: what the app answers from `cmux vault … --json`
// (cmux-tui cli/vault.rs). It holds names and scopes only; a value the page sends is recorded once
// (`sent`) so tests can check it went to the host and nowhere else, then dropped from the state.
import { ProtocolError } from "../../protocol/errors";
import type {
  VaultDeviceWire,
  VaultItemWire,
  VaultKept,
  VaultPreview,
  VaultPreviewRow,
  VaultRun,
  VaultStatusWire,
} from "./ops";

const MASK = "••••••••";

export function mockVaultDevice(patch: Partial<VaultDeviceWire> & { id: string }): VaultDeviceWire {
  return { kind: "mac", label: "Studio Mac", state: "active", safetyCode: "40218837", thisDevice: false, ...patch };
}

export function mockVaultItem(name: string, scope = "*", patch: Partial<VaultItemWire> = {}): VaultItemWire {
  const label = scope === "*" ? "every machine" : scope.replace(":", " ");
  return {
    id: `it_${name.toLowerCase()}_${scope.length}`,
    name,
    scope,
    scopeLabel: label,
    revision: 1,
    updatedAt: 0,
    masked: MASK,
    ...patch,
  };
}

/** The vault on with this Mac, an iPhone waiting for an approve and four items. */
export function mockVaultOn(): VaultStatusWire {
  return {
    vault: "personal",
    state: "active",
    epoch: 2,
    deviceId: "dv_thismac",
    safetyCode: "51830264",
    devices: [
      mockVaultDevice({ id: "dv_thismac", label: "MacBook Pro", safetyCode: "51830264", thisDevice: true }),
      mockVaultDevice({ id: "dv_iphone", kind: "iphone", label: "iPhone", state: "pending", safetyCode: "77314092" }),
      mockVaultDevice({ id: "dv_studio", label: "Studio Mac", safetyCode: "40218837" }),
    ],
    items: [
      mockVaultItem("OPENAI_API_KEY"),
      mockVaultItem("DATABASE_URL", "repo:example/web"),
      mockVaultItem("STRIPE_SECRET_KEY", "repo:example/web"),
      mockVaultItem("OPENAI_API_KEY", "machine:vm_0a1b2c3d4e5f6g7h8i9j"),
    ],
  };
}

/** This Mac asked to join and waits for an approve on another device. */
export function mockVaultPending(): VaultStatusWire {
  return {
    vault: "personal",
    state: "pending",
    epoch: 1,
    deviceId: "dv_thismac",
    safetyCode: "51830264",
    devices: [
      mockVaultDevice({ id: "dv_studio", label: "Studio Mac", safetyCode: "40218837" }),
      mockVaultDevice({
        id: "dv_thismac",
        label: "MacBook Pro",
        state: "pending",
        safetyCode: "51830264",
        thisDevice: true,
      }),
    ],
    items: [],
  };
}

/** A picked `.env`'s names as `cmux vault import FILE` previews them. */
export function mockVaultPreview(): VaultPreview {
  const row = (name: string, line: number, keepable = true, alreadyKept = false): VaultPreviewRow => ({
    name,
    line,
    keepable,
    alreadyKept,
    masked: MASK,
  });
  return {
    token: "tok_env",
    file: ".env.local",
    preview: [
      row("OPENAI_API_KEY", 1, true, true),
      row("ANTHROPIC_API_KEY", 2),
      row("DATABASE_URL", 4),
      row("PATH", 6, false),
      row("CMUX_SOCKET_PATH", 7, false),
      row("SENTRY_DSN", 9),
    ],
    skippedLines: [11],
  };
}

export class MockVault {
  /** What `state` answers; null: the host refuses as signed out. */
  status: VaultStatusWire | null = mockVaultOn();
  /** What the picker returns; null: the person cancelled the panel. */
  picked: VaultPreview | null = mockVaultPreview();
  /** The host's refusal of the next gesture (tests), else null. */
  refusal: string | null = null;
  /** Every gesture the page sent, a `set` value included (the real host pipes it on). */
  sent: VaultRun[] = [];
  kept: { token: string; names: string[]; scope: string }[] = [];

  state(): VaultStatusWire {
    if (!this.status) throw new ProtocolError("cmux.vault.not_signed_in", "no Cloud sign-in");
    return this.status;
  }

  run(run: VaultRun): Record<string, unknown> {
    this.sent.push(run);
    if (this.refusal) {
      const refusal = this.refusal;
      this.refusal = null;
      throw new ProtocolError("cmux.vault.refused", refusal);
    }
    const status = this.status ?? mockVaultPending();
    switch (run.action) {
      case "on":
        this.status = status.state === "active" ? status : { ...mockVaultPending() };
        return {};
      case "approve":
        this.status = {
          ...status,
          devices: status.devices.map((d) => (d.safetyCode === run.code ? { ...d, state: "active" } : d)),
        };
        return {};
      case "remove_device":
        this.status = { ...status, devices: status.devices.filter((d) => d.id !== run.id), epoch: status.epoch + 1 };
        return {};
      case "set": {
        const items = status.items.filter(
          (i) => !(i.name === run.name && (i.scope === run.scope || i.scope === run.replace_scope)),
        );
        this.status = { ...status, items: [...items, mockVaultItem(run.name, run.scope)] };
        return { name: run.name, scope: run.scope, revision: 2 };
      }
      case "remove":
        this.status = { ...status, items: status.items.filter((i) => !(i.name === run.name && i.scope === run.scope)) };
        return { removed: 1 };
      default:
        return { machines: [] };
    }
  }

  preview(scope: string, token?: string): VaultPreview {
    if (!this.picked) return { cancelled: true };
    if (token && token !== this.picked.token)
      throw new ProtocolError("cmux.vault.import_gone", "Choose the file again.");
    const kept = new Set((this.status?.items ?? []).filter((i) => i.scope === scope).map((i) => i.name));
    return {
      ...this.picked,
      preview: (this.picked.preview ?? []).map((row) => ({ ...row, alreadyKept: kept.has(row.name) })),
    };
  }

  keep(params: { token: string; names: string[]; scope: string }): VaultKept {
    this.kept.push(params);
    const status = this.status ?? mockVaultOn();
    const items = status.items.filter((i) => !(params.names.includes(i.name) && i.scope === params.scope));
    this.status = { ...status, items: [...items, ...params.names.map((name) => mockVaultItem(name, params.scope))] };
    return { kept: params.names, scope: params.scope, machines: 2 };
  }
}
