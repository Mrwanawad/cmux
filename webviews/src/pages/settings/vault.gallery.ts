// l10n-allow-file: gallery fixtures (sample device labels, names and repositories), not shipped UI.
// Settings > Cloud > Vault (VaultCard, cmuxterm-hq docs/design/e2e-vault.md): off with one Turn On,
// this Mac waiting with its 8-digit code, on with devices and masked items, Add, Approve and Import
// from .env with a checkbox per name. No fixture holds a value.
import { settingsPageEntry, type PageFixtureStep, type SettingsPageVariant } from "../../gallery/format";
import { mockVaultOn, mockVaultPending } from "./mockVault";
import { VAULT_SETTING } from "./vault";

const shown: PageFixtureStep = { selector: '[data-card="vault"]', action: "wait" };
const on = { values: { [VAULT_SETTING]: true } };
const variant = (patch: Partial<SettingsPageVariant> & { note: string }): SettingsPageVariant => ({
  section: "cloud",
  focus: "card.vault",
  steps: [shown],
  ...patch,
});

export default settingsPageEntry({
  id: "pages.settings-vault",
  title: "Settings Vault",
  area: "Settings",
  height: 720,
  covers: [
    "pages/settings/components/VaultCard.tsx#VaultCard",
    "pages/settings/components/VaultImport.tsx#VaultImport",
  ],
  anchors: [{ selector: '[data-card="vault"]' }],
  variants: {
    off: variant({
      note: "Off (the default): one line on what the vault is and one Turn On.",
    }),
    pending: variant({
      options: on,
      vault: mockVaultPending(),
      note: "This Mac asked to join: its 8-digit code to compare on a device already in the vault.",
    }),
    on: variant({
      options: on,
      note: "On: this Mac, a waiting iPhone with Approve, another Mac; four items masked with their scopes.",
    }),
    approve: variant({
      options: on,
      steps: [shown, { selector: '[data-vault-device="dv_iphone"] [data-vault-action="approve"]', action: "click" }],
      note: "Approve asks first and shows the iPhone's code to compare.",
    }),
    add: variant({
      options: on,
      steps: [
        shown,
        { selector: '[data-vault-action="add"]', action: "click" },
        { selector: '[data-vault-field="name"]', action: "input", value: "SENTRY_DSN" },
        { selector: '[data-vault-field="value"]', action: "input", value: "example-value" },
      ],
      note: "Add: a name, the value in a password field, and where it goes.",
    }),
    import: variant({
      options: on,
      steps: [
        shown,
        { selector: '[data-vault-action="import"]', action: "click" },
        { selector: '[data-vault-action="choose-file"]', action: "click" },
        { selector: "[data-vault-import-names]", action: "wait" },
      ],
      note: "Import from .env: every name ticked, PATH and CMUX_SOCKET_PATH refused and off, one already kept marked.",
    }),
    "signed-out": variant({
      options: on,
      vault: null,
      note: "On, but cmux is signed out: one line and Try Again.",
    }),
    empty: variant({
      options: on,
      vault: { ...mockVaultOn(), items: [] },
      note: "On with nothing kept yet.",
    }),
  },
});
