// Settings > Cloud > Vault (cmuxterm-hq docs/design/e2e-vault.md): off, one Turn On; this Mac waiting
// with its 8-digit code; on with devices (Approve after comparing codes, Remove) and items masked with
// their scope; Add sends the value once and the field empties; Import from .env shows names only, a
// checkbox per name, the refused ones off. No value is ever on the page.
import { afterAll, afterEach, expect, test } from "bun:test";
import { installDom } from "./testDom";
import type { MockSettingsProvider } from "./mockProvider";
import type { Rendered } from "./testing";
import { installVirtualSizes } from "../../../test/support/virtualSizes";
import { settingsCardSizes } from "../../../test/support/settingsCardSizes";

const restore = installDom();
afterAll(() => restore());
installVirtualSizes({ HTMLElement: globalThis.window.HTMLElement }, settingsCardSizes);
const { renderPage, settle, click, changeValue } = await import("./testing");
const { VAULT_SETTING, codeText, deviceRows, itemRows, scopeWire, scopeChoice, vaultPhase, plausibleName } =
  await import("./vault");
const { mockVaultOn, mockVaultPending } = await import("./mockVault");

type VaultStatus = ReturnType<typeof mockVaultOn>;
const ok = (status: VaultStatus) => ({ status, error: null, code: null });

let page: Rendered | null = null;
afterEach(() => {
  page?.unmount();
  page = null;
});

const card = () => page!.container.querySelector<HTMLElement>('[data-card="vault"]')!;
const action = (name: string, within: ParentNode = card()) =>
  within.querySelector<HTMLButtonElement>(`[data-vault-action="${name}"]`)!;

async function open(enabled: boolean, setup?: (provider: MockSettingsProvider) => void) {
  page = await renderPage({ path: "/settings/cloud", mock: { values: { [VAULT_SETTING]: enabled } }, setup });
  await settle();
  await settle();
  return page;
}

test("phase, code and row order come from the CLI's answer", () => {
  expect(vaultPhase(false, null)).toBe("off");
  expect(vaultPhase(true, null)).toBe("loading");
  expect(vaultPhase(true, { status: null, error: "x", code: "cmux.vault.not_signed_in" })).toBe("signedOut");
  expect(vaultPhase(true, { status: null, error: "x", code: "cmux.vault.failed" })).toBe("error");
  expect(vaultPhase(true, ok(mockVaultPending()))).toBe("pending");
  expect(vaultPhase(true, ok({ ...mockVaultOn(), state: "not_enrolled" }))).toBe("joining");
  expect(vaultPhase(true, ok(mockVaultOn()))).toBe("on");
  expect(codeText("77314092")).toBe("7731 4092");
  const read = ok(mockVaultOn());
  expect(deviceRows(read).map((d) => d.id)).toEqual(["dv_iphone", "dv_thismac", "dv_studio"]);
  expect(itemRows(read).map((i) => `${i.name} ${i.scope}`)).toEqual([
    "DATABASE_URL repo:example/web",
    "OPENAI_API_KEY *",
    "OPENAI_API_KEY machine:vm_0a1b2c3d4e5f6g7h8i9j",
    "STRIPE_SECRET_KEY repo:example/web",
  ]);
});

test("the scope picker's choice becomes the CLI's wire scope, or nothing until it is filled in", () => {
  expect(scopeWire({ kind: "all", target: "" })).toBe("*");
  expect(scopeWire({ kind: "repo", target: " Example/Web " })).toBe("repo:example/web");
  expect(scopeWire({ kind: "repo", target: "example" })).toBeNull();
  expect(scopeWire({ kind: "machine", target: "vm_0a1b2c3d4e5f6g7h8i9j" })).toBe("machine:vm_0a1b2c3d4e5f6g7h8i9j");
  expect(scopeWire({ kind: "machine", target: "vm_short" })).toBeNull();
  expect(scopeChoice("repo:example/web")).toEqual({ kind: "repo", target: "example/web" });
  expect(scopeChoice("*")).toEqual({ kind: "all", target: "" });
  expect(plausibleName("OPENAI_API_KEY")).toBe(true);
  expect(plausibleName("1BAD")).toBe(false);
});

test("off: one line and Turn On, and the vault is not read", async () => {
  await open(false);
  expect(card().querySelector("[data-vault-off]")).not.toBeNull();
  expect(page!.provider.vault.sent).toEqual([]);
  await click(action("on"));
  await settle();
  await settle();
  expect(page!.provider.vault.sent.map((run) => run.action)).toEqual(["on"]);
});

test("pending: this Mac's code to compare on another device", async () => {
  await open(true, (provider) => {
    provider.vault.status = mockVaultPending();
  });
  expect(card().querySelector("[data-vault-code]")!.textContent).toContain("5183 0264");
});

test("on: devices and items masked, never a value; Approve asks first and shows the code", async () => {
  await open(true);
  const iphone = card().querySelector<HTMLElement>('[data-vault-device="dv_iphone"]')!;
  expect(iphone).not.toBeNull();
  expect(card().textContent).toContain("••••••••");
  await click(action("approve", iphone));
  expect(iphone.textContent).toContain("7731 4092");
  expect(page!.provider.vault.sent).toEqual([]);
  await click(action("confirm-approve", iphone));
  await settle();
  expect(page!.provider.vault.sent).toEqual([{ action: "approve", code: "77314092" }]);
});

test("Add sends the value once, the field empties, and the page never shows it", async () => {
  await open(true);
  await click(action("add"));
  await changeValue(card().querySelector<HTMLInputElement>('[data-vault-field="name"]')!, "SENTRY_DSN");
  const value = card().querySelector<HTMLInputElement>('[data-vault-field="value"]')!;
  expect(value.type).toBe("password");
  await changeValue(value, "example-value");
  await click(action("save"));
  await settle();
  await settle();
  expect(page!.provider.vault.sent).toEqual([
    { action: "set", name: "SENTRY_DSN", value: "example-value", scope: "*" },
  ]);
  expect(page!.container.innerHTML).not.toContain("example-value");
  expect(card().textContent).toContain("SENTRY_DSN");
});

test("Import from .env: names only, all ticked, refused ones off; Keep sends the ticked names", async () => {
  await open(true);
  await click(action("import"));
  await click(action("choose-file"));
  await settle();
  const names = card().querySelector<HTMLElement>("[data-vault-import-names]")!;
  const box = (name: string) =>
    names.querySelector<HTMLInputElement>(`[data-vault-import-name="${name}"] input[type="checkbox"]`)!;
  expect(box("ANTHROPIC_API_KEY").checked).toBe(true);
  expect(box("PATH").checked).toBe(false);
  expect(box("PATH").disabled).toBe(true);
  await click(box("SENTRY_DSN"));
  await click(action("keep"));
  await settle();
  await settle();
  expect(page!.provider.vault.kept).toEqual([
    { token: "tok_env", names: ["OPENAI_API_KEY", "ANTHROPIC_API_KEY", "DATABASE_URL"], scope: "*" },
  ]);
});

test("signed out: one line and Try Again", async () => {
  await open(true, (provider) => {
    provider.vault.status = null;
  });
  expect(card().querySelector('[data-vault-action="retry"]')).not.toBeNull();
});
