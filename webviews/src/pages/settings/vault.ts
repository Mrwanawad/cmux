// Settings > Cloud > Vault (cmuxterm-hq docs/design/e2e-vault.md): what the card draws from
// `cmux.settings.vault.state`, which is `cmux vault status --json` (cmux-tui cli/vault.rs, crate
// cmux-vault-e2e). Every rule of the vault (names it keeps, scopes, the crypto, the import parse)
// is Rust's; this file only reads its answer defensively and orders the rows. No answer carries a
// value: an item is its name, its masked `••••••••` and where it goes.
import type { VaultDeviceWire, VaultItemWire, VaultRead } from "./ops";

/** What the card shows. `off`: the setting is off (Turn On). `joining`: on, this Mac not in the
 * vault yet. `pending`: this Mac waits for an approve (its code). `on`: devices and items. */
export type VaultPhase = "off" | "loading" | "signedOut" | "error" | "joining" | "pending" | "on";

/** The setting the card turns on and off (schemas/settings/source/machines.json). */
export const VAULT_SETTING = "cloud.vault.enabled";

export function vaultPhase(enabled: boolean, read: VaultRead | null | undefined): VaultPhase {
  if (!enabled) return "off";
  if (!read) return "loading";
  const status = read.status;
  if (!status) return read.code?.endsWith("not_signed_in") ? "signedOut" : "error";
  switch (status.state) {
    case "active":
      return "on";
    case "pending":
      return "pending";
    case "off":
    case "not_enrolled":
      return "joining";
    default:
      return "error";
  }
}

/** An 8-digit safety code as the person compares it: `1234 5678`. */
export function codeText(code: string | null | undefined): string {
  const digits = (code ?? "").replace(/\D/g, "");
  return digits.length === 8 ? `${digits.slice(0, 4)} ${digits.slice(4)}` : digits;
}

/** Devices: one waiting for an approve first (it needs the person), then this Mac, then by label. */
export function deviceRows(read: VaultRead | null | undefined): VaultDeviceWire[] {
  const rank = (device: VaultDeviceWire) => (device.state === "pending" ? 0 : device.thisDevice ? 1 : 2);
  return (read?.status?.devices ?? [])
    .filter((device) => typeof device?.id === "string" && device.id !== "")
    .slice()
    .sort((a, b) => rank(a) - rank(b) || (a.label ?? "").localeCompare(b.label ?? "") || a.id.localeCompare(b.id));
}

const SCOPE_RANK: Record<ScopeKind, number> = { all: 0, repo: 1, machine: 2 };

/** Items by name, then from the widest scope to the narrowest (the narrowest wins on a machine). */
export function itemRows(read: VaultRead | null | undefined): VaultItemWire[] {
  return (read?.status?.items ?? [])
    .filter((item) => typeof item?.name === "string" && item.name !== "")
    .slice()
    .sort(
      (a, b) =>
        a.name.localeCompare(b.name) ||
        SCOPE_RANK[scopeChoice(a.scope).kind] - SCOPE_RANK[scopeChoice(b.scope).kind] ||
        a.scope.localeCompare(b.scope),
    );
}

export type ScopeKind = "all" | "repo" | "machine";
/** Where a value goes, as the scope picker holds it; `target` is OWNER/NAME or a machine id. */
export type ScopeChoice = { kind: ScopeKind; target: string };

/** A wire scope (`*`, `repo:OWNER/NAME`, `machine:vm_…`) as the picker's choice. */
export function scopeChoice(wire: string | null | undefined): ScopeChoice {
  if (wire?.startsWith("repo:")) return { kind: "repo", target: wire.slice(5) };
  if (wire?.startsWith("machine:")) return { kind: "machine", target: wire.slice(8) };
  return { kind: "all", target: "" };
}

/** The picker's choice as the wire scope the CLI takes, or null while the target is not filled in.
 * The CLI checks its shape again; this only keeps Save off until there is something to send. */
export function scopeWire(choice: ScopeChoice): string | null {
  const target = choice.target.trim();
  if (choice.kind === "all") return "*";
  if (choice.kind === "repo") return /^[^/\s]+\/[^/\s]+$/.test(target) ? `repo:${target.toLowerCase()}` : null;
  return /^vm_[a-z0-9]{20}$/.test(target) ? `machine:${target}` : null;
}

/** A name the field may send: the environment-name shape (the CLI refuses the names cmux and the
 * shell own, and says so). */
export function plausibleName(name: string): boolean {
  return /^[A-Za-z_][A-Za-z0-9_]{0,127}$/.test(name);
}
