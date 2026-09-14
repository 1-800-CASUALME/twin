import SwiftUI

struct SyncView: View {
    @Environment(AppState.self) private var state

    var body: some View {
        VStack(spacing: 16) {
            HStack {
                Text("Sync").font(.title2.weight(.semibold))
                Spacer()
                BigButton(title: state.synced ? "Sync again" : "Sync", symbol: "arrow.triangle.2.circlepath", busy: state.busy) {
                    Task { await state.sync() }
                }
                .controlSize(.regular)
            }
            if state.stepStates.isEmpty && !state.busy {
                Spacer()
                SymbolIcon(name: "arrow.triangle.2.circlepath", size: 56)
                Text("Ready. \(state.selectedItems.count) items selected.").foregroundStyle(.secondary)
                Spacer()
            } else {
                ScrollView {
                    VStack(spacing: 10) {
                        ForEach(state.selectedItems.sorted(), id: \.self) { id in ItemProgressRow(id: id) }
                        if !state.conflicts.isEmpty {
                            VStack(alignment: .leading, spacing: 4) {
                                Label("\(state.conflicts.count) conflicts kept as copies", systemImage: "doc.on.doc").font(.headline)
                                ForEach(state.conflicts) { c in
                                    Text(c.path).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1)
                                }
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .card()
                        }
                    }
                }
            }
        }
        .padding(24)
    }
}

struct ItemProgressRow: View {
    @Environment(AppState.self) private var state
    let id: String
    var icon: String { state.items.first { $0.id == id }?.icon ?? "circle" }
    var body: some View {
        let st = state.stepStates[id]
        let pr = state.progress[id]
        let subSteps = state.stepStates.filter { $0.key.hasPrefix(id + ":") }.sorted { $0.key < $1.key }
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 12) {
                SymbolIcon(name: icon, size: 26, pulse: st?.0 == .running)
                VStack(alignment: .leading, spacing: 2) {
                    Text(state.displayName(id)).font(.headline)
                    Text(st?.1 ?? "Waiting…").font(.caption).foregroundStyle(.secondary).lineLimit(2)
                }
                Spacer()
                StatusDot(state: st?.0 ?? .pending)
            }
            if let pr, st?.0 == .running, pr.1 > 0 {
                ProgressView(value: Double(pr.0), total: Double(pr.1))
            }
            if !subSteps.isEmpty {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(subSteps, id: \.key) { k, v in
                        HStack(spacing: 6) {
                            StatusDot(state: v.0)
                            Text(String(k.dropFirst(id.count + 1))).font(.caption.monospaced())
                            Text(v.1).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                        }
                    }
                }
                .padding(.leading, 38)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .card()
    }
}
