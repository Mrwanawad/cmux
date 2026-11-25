// Settings > Cloud > Vault: where a value goes, for a person and as the scope picker the Add, Edit
// and Import forms share (every machine, machines from one repository, one machine).
import { Input } from "../../../ui/Input";
import { Select } from "../../../ui/Select";
import { t } from "../strings";
import { scopeChoice, type ScopeChoice } from "../vault";

/** Where a value goes, for a person. */
export function scopeLabel(wire: string): string {
  const choice = scopeChoice(wire);
  if (choice.kind === "repo") return t("settingsPage.vault.scopeRepo", choice.target);
  if (choice.kind === "machine") return t("settingsPage.vault.scopeMachine", choice.target);
  return t("settingsPage.vault.scopeAll");
}

/** Every machine, machines from one repository (OWNER/NAME), or one machine (its id). */
export function ScopePicker({ scope, onChange }: { scope: ScopeChoice; onChange: (scope: ScopeChoice) => void }) {
  const options = [
    { value: "all", label: t("settingsPage.vault.scopeAll") },
    { value: "repo", label: t("settingsPage.vault.scopeRepoPick") },
    { value: "machine", label: t("settingsPage.vault.scopeMachinePick") },
  ];
  return (
    <div className="host-field vault-scope" data-vault-scope-picker="">
      <span className="row-help">{t("settingsPage.vault.scope")}</span>
      <span className="vault-scope-controls">
        <Select
          label={t("settingsPage.vault.scope")}
          value={scope.kind}
          options={options}
          closeOnPick
          onChange={(kind) =>
            onChange({ kind: kind as ScopeChoice["kind"], target: kind === scope.kind ? scope.target : "" })
          }
        />
        {scope.kind !== "all" && (
          <Input
            className="field font-mono"
            data-vault-field="scope-target"
            aria-label={
              scope.kind === "repo" ? t("settingsPage.vault.scopeRepoPick") : t("settingsPage.vault.scopeMachinePick")
            }
            placeholder={scope.kind === "repo" ? "owner/name" : "vm_…"}
            spellCheck={false}
            autoComplete="off"
            value={scope.target}
            onChange={(event) => onChange({ ...scope, target: event.target.value })}
          />
        )}
      </span>
    </div>
  );
}
