import SwiftUI

let checkIcons: [String: String] = [
    "ssh": "network", "rsync": "arrow.left.arrow.right", "git": "arrow.triangle.branch", "tmux": "terminal",
    "et": "bolt.horizontal", "atuin": "clock.arrow.circlepath", "chezmoi": "doc.text", "claude": "sparkles",
    "disk": "internaldrive", "conflicts": "exclamationmark.triangle", "twin": "arrow.triangle.2.circlepath",
]

struct DiagnoseView: View {
    @Environment(AppState.self) private var state
    private let columns = [GridItem(.adaptive(minimum: 160), spacing: 10)]

    var body: some View {
        VStack(spacing: 14) {
            HStack {
                Text("Both machines").font(.title2.weight(.semibold))
                Spacer()
                BigButton(title: state.diagnosed ? "Check again" : "Check", symbol: "stethoscope", busy: state.busy) {
                    Task { await state.diagnose() }
                }
                .controlSize(.regular)
            }
            if state.checks.isEmpty && state.busy {
                Spacer(); ProgressView("Checking…"); Spacer()
            } else {
                ScrollView {
                    LazyVGrid(columns: columns, spacing: 10) {
                        ForEach(state.sortedChecks) { c in CheckCard(check: c) }
                    }
                }
            }
        }
        .padding(24)
    }
}

struct CheckCard: View {
    @Environment(AppState.self) private var state
    let check: CheckResult
    var sideLabel: String { check.side == "local" ? "This Mac" : (check.side == "peer" ? "Other" : "Pair") }
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                SymbolIcon(name: checkIcons[check.id] ?? "questionmark.circle", size: 22, pulse: check.state == .running)
                Spacer()
                StatusDot(state: check.state)
            }
            Text(check.name).font(.headline).lineLimit(1)
            Text(sideLabel).font(.caption2.weight(.medium)).foregroundStyle(.secondary)
                .padding(.horizontal, 6).padding(.vertical, 2)
                .background(.quaternary, in: Capsule())
            Text(check.msg).font(.caption).foregroundStyle(.secondary).lineLimit(2)
            if check.fixable {
                Button("Fix") { Task { await state.fix(check) } }.controlSize(.small).disabled(state.busy)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
    }
}
