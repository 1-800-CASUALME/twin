import Foundation
import SwiftUI

enum Step: Int, CaseIterable, Identifiable {
    case welcome, connect, diagnose, choose, sync, done
    var id: Int { rawValue }
    var title: String {
        switch self {
        case .welcome: "Welcome"; case .connect: "Connect"; case .diagnose: "Diagnose"
        case .choose: "Choose"; case .sync: "Sync"; case .done: "Done"
        }
    }
    var symbol: String {
        switch self {
        case .welcome: "hand.wave"; case .connect: "link"; case .diagnose: "stethoscope"
        case .choose: "checklist"; case .sync: "arrow.triangle.2.circlepath"; case .done: "checkmark.seal"
        }
    }
}

struct PeerStatus: Hashable { var name: String; var addr: String }

/// Cards that are not backed by an engine yet. Shown disabled with a "Soon" badge.
struct PlaceholderItem: Identifiable { let id: String; let name: String; let icon: String }
let placeholderItems: [PlaceholderItem] = [
    .init(id: "dotfiles", name: "Dotfiles", icon: "doc.text"),
    .init(id: "history", name: "History", icon: "clock.arrow.circlepath"),
    .init(id: "terminal", name: "Terminal", icon: "terminal"),
    .init(id: "folders", name: "Folders", icon: "folder"),
]

@MainActor @Observable
final class AppState {
    var step: Step = .welcome
    var busy = false
    var error: String?

    // Connect
    var peers: [PeerInfo] = []
    var pairingCode: String?
    var paired: PeerStatus?
    var pairingPeer: PeerInfo?
    private var pairHandle: ProcessHandle?
    private var daemonHandle: ProcessHandle?
    private var daemonAwaitingConfirm = false

    // Diagnose
    var checks: [String: CheckResult] = [:]
    var diagnosed = false

    // Choose
    var items: [Item] = []
    var selectedItems: Set<String> = []
    var selectedMembers: Set<String> = []   // "item:member"
    var autocommitRepos: Set<String> = []

    // Sync
    var stepStates: [String: (StepState, String)] = [:]
    var progress: [String: (Int64, Int64)] = [:]
    var conflicts: [Conflict] = []
    var synced = false
    var syncOK = false

    var sortedChecks: [CheckResult] {
        let order = ["local": 0, "pair": 1, "peer": 2]
        return checks.values.sorted { (order[$0.side] ?? 9, $0.id) < (order[$1.side] ?? 9, $1.id) }
    }

    var canContinue: Bool {
        switch step {
        case .welcome: true
        case .connect: paired != nil
        case .diagnose: diagnosed && !checks.values.contains { $0.state == .fail }
        case .choose: !selectedItems.isEmpty
        case .sync: synced
        case .done: false
        }
    }

    func onLaunch() {
        TwinCore.ensureUserSymlink()
        Task { await loadStatus() }
        startDaemon()
    }

    func next() { if let n = Step(rawValue: step.rawValue + 1) { step = n; onEnter(n) } }
    func back() { if let p = Step(rawValue: step.rawValue - 1) { step = p } }
    func restart() { synced = false; syncOK = false; stepStates = [:]; progress = [:]; conflicts = []; step = .choose; onEnter(.choose) }

    private func onEnter(_ s: Step) {
        switch s {
        case .diagnose where !diagnosed: Task { await diagnose() }
        case .choose where items.isEmpty: Task { await inventory() }
        default: break
        }
    }

    // MARK: connect

    func loadStatus() async {
        let (events, _) = TwinCore.run(["status"])
        for await ev in events {
            if case .step(_, let st, let msg) = ev, st == .ok || st == .warn {
                let name = msg.components(separatedBy: " (").first ?? msg
                let addr = msg.components(separatedBy: "(").dropFirst().first?.components(separatedBy: ")").first ?? ""
                paired = PeerStatus(name: name, addr: addr)
            }
        }
    }

    /// Advertise this Mac so the other machine can find and pair with it.
    func startDaemon() {
        guard daemonHandle == nil else { return }
        let (events, handle) = TwinCore.run(["daemon"])
        daemonHandle = handle
        Task {
            for await ev in events {
                switch ev {
                case .code(let c): pairingCode = c; daemonAwaitingConfirm = true
                case .step("pair", .ok, let msg):
                    pairingCode = nil; daemonAwaitingConfirm = false
                    paired = PeerStatus(name: msg.replacingOccurrences(of: "paired with ", with: ""), addr: "")
                    await loadStatus()
                case .step("pair", .fail, let msg): pairingCode = nil; daemonAwaitingConfirm = false; error = msg
                default: break
                }
            }
            daemonHandle = nil
        }
    }

    func discover() async {
        busy = true; peers = []; error = nil
        let (events, _) = TwinCore.run(["discover", "--timeout", "4"])
        for await ev in events {
            if case .peer(let p) = ev, !peers.contains(where: { $0.instance == p.instance }) { peers.append(p) }
            if case .error(let m) = ev { error = m }
        }
        busy = false
    }

    func pair(_ peer: PeerInfo) {
        pairingPeer = peer; error = nil
        let (events, handle) = TwinCore.run(["pair", "--addr", peer.addr, "--port", String(peer.port)])
        pairHandle = handle
        Task {
            for await ev in events {
                switch ev {
                case .code(let c): pairingCode = c
                case .step("pair", .ok, _): pairingCode = nil; await loadStatus()
                case .error(let m): error = m; pairingCode = nil
                default: break
                }
            }
            pairHandle = nil; pairingPeer = nil
        }
    }

    func confirmCode(_ yes: Bool) {
        let answer = yes ? "y" : "n"
        if daemonAwaitingConfirm { daemonHandle?.write(answer); daemonAwaitingConfirm = false }
        pairHandle?.write(answer)
        if !yes { pairingCode = nil }
    }

    // MARK: diagnose

    func diagnose() async {
        busy = true; checks = [:]; error = nil
        let (events, _) = TwinCore.run(["diagnose"])
        for await ev in events {
            switch ev {
            case .check(let c): checks[c.key] = c
            case .error(let m): error = m
            default: break
            }
        }
        diagnosed = true; busy = false
    }

    func fix(_ c: CheckResult) async {
        if c.id == "ssh" && c.side == "local" {
            NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preferences.sharing?Services_RemoteLogin")!)
            return
        }
        if c.side == "peer" { error = "Fix \(c.name) on the other machine, then check again."; return }
        busy = true
        checks[c.key]?.state = .running
        let (events, _) = TwinCore.run(["fix", c.id])
        for await ev in events { if case .error(let m) = ev { error = m } }
        busy = false
        await diagnose()
    }

    // MARK: choose

    func inventory() async {
        busy = true; items = []; error = nil
        let (events, _) = TwinCore.run(["inventory"])
        for await ev in events {
            switch ev {
            case .item(let i): items.append(i)
            case .error(let m): error = m
            default: break
            }
        }
        if selectedItems.isEmpty { selectAll(true) }
        busy = false
    }

    func isSelected(_ item: Item) -> Bool { selectedItems.contains(item.id) }
    func isSelected(_ item: Item, _ m: Member) -> Bool { selectedMembers.contains("\(item.id):\(m.id)") }

    func toggle(_ item: Item) {
        if selectedItems.contains(item.id) {
            selectedItems.remove(item.id)
            for m in item.members { selectedMembers.remove("\(item.id):\(m.id)") }
        } else {
            selectedItems.insert(item.id)
            for m in item.members { selectedMembers.insert("\(item.id):\(m.id)") }
        }
    }

    func toggle(_ item: Item, _ m: Member) {
        let k = "\(item.id):\(m.id)"
        if selectedMembers.contains(k) { selectedMembers.remove(k) } else { selectedMembers.insert(k) }
        let any = item.members.contains { selectedMembers.contains("\(item.id):\($0.id)") }
        if any { selectedItems.insert(item.id) } else { selectedItems.remove(item.id) }
    }

    var allSelected: Bool { !items.isEmpty && items.allSatisfy { i in i.members.allSatisfy { isSelected(i, $0) } } }

    func selectAll(_ on: Bool) {
        selectedItems = []; selectedMembers = []
        guard on else { return }
        for i in items { selectedItems.insert(i.id); for m in i.members { selectedMembers.insert("\(i.id):\(m.id)") } }
    }

    func toggleAutocommit(_ m: Member) {
        if autocommitRepos.contains(m.id) { autocommitRepos.remove(m.id) } else { autocommitRepos.insert(m.id) }
    }

    // MARK: sync

    func sync() async {
        busy = true; error = nil; stepStates = [:]; progress = [:]; conflicts = []; synced = false
        // persist the selection, then run it
        var sel = ["select"] + selectedItems.sorted()
        for i in items where selectedItems.contains(i.id) {
            let all = i.members.allSatisfy { isSelected(i, $0) }
            if !all { for m in i.members where isSelected(i, m) { sel += ["--member", "\(i.id):\(m.id)"] } }
        }
        for r in autocommitRepos.sorted() { sel += ["--autocommit", r] }
        let (selEvents, _) = TwinCore.run(sel)
        for await _ in selEvents {}
        let (events, _) = TwinCore.run(["sync", "--all"])
        for await ev in events {
            switch ev {
            case .step(let id, let st, let msg): stepStates[id] = (st, msg)
            case .progress(let id, let done, let total, _): progress[id] = (done, total)
            case .conflict(let id, let path, let kept): conflicts.append(Conflict(item: id, path: path, kept: kept))
            case .error(let m): error = m
            case .done(let ok): syncOK = ok
            default: break
            }
        }
        synced = true; busy = false
    }

    var summaryLines: [String] {
        selectedItems.sorted().compactMap { id in stepStates[id].map { "\(displayName(id)): \($0.1)" } }
    }
    func displayName(_ id: String) -> String { items.first { $0.id == id }?.name ?? id.capitalized }
}
