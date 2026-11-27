import Foundation
import CmuxNextActions

/// SCRATCH ONLY (task a136 measurement): per-action and per-part times of the Cmd-Shift-P model stage, printed to
/// stderr after each open. Never lands.
enum ScratchOpenProfile {
    static var perAction: [ActionID: (n: Int, ns: UInt64)] = [:]
    static var parts: [String: (n: Int, ns: UInt64)] = [:]
    static var opens = 0

    static func now() -> UInt64 { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) }

    static func add(_ id: ActionID, _ ns: UInt64) {
        let old = perAction[id] ?? (0, 0)
        perAction[id] = (old.n + 1, old.ns + ns)
    }

    static func part(_ name: String, _ ns: UInt64) {
        let old = parts[name] ?? (0, 0)
        parts[name] = (old.n + 1, old.ns + ns)
    }

    @discardableResult
    static func time<T>(_ name: String, _ body: () -> T) -> T {
        let start = now()
        let value = body()
        part(name, now() - start)
        return value
    }

    static func dump() {
        opens += 1
        guard opens % 10 == 0 else { return }
        var out = "SCRATCH-PROFILE opens=\(opens)\n"
        for (name, v) in parts.sorted(by: { $0.value.ns > $1.value.ns }) {
            out += String(format: "SCRATCH-PART %@ n=%d total_ms=%.2f per_open_ms=%.3f\n", name, v.n, Double(v.ns) / 1e6,
                          Double(v.ns) / 1e6 / Double(max(1, opens)))
        }
        let total = perAction.values.reduce(UInt64(0)) { $0 + $1.ns }
        out += String(format: "SCRATCH-ACTIONS count=%d total_ms=%.2f per_open_ms=%.3f\n", perAction.count, Double(total) / 1e6,
                      Double(total) / 1e6 / Double(max(1, opens)))
        for (id, v) in perAction.sorted(by: { $0.value.ns > $1.value.ns }).prefix(40) {
            out += String(format: "SCRATCH-ACTION %@ n=%d per_open_us=%.1f\n", id.rawValue, v.n, Double(v.ns) / 1e3 / Double(max(1, opens)))
        }
        FileHandle.standardError.write(Data(out.utf8))
    }
}
