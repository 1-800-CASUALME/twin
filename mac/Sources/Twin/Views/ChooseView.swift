import SwiftUI

struct ChooseView: View {
    @Environment(AppState.self) private var state
    private let columns = [GridItem(.adaptive(minimum: 220), spacing: 12)]

    var body: some View {
        VStack(spacing: 12) {
            HStack {
                Text("What to sync").font(.title2.weight(.semibold))
                Spacer()
                Toggle("Select All", isOn: Binding(get: { state.allSelected }, set: { state.selectAll($0) }))
                    .toggleStyle(.checkbox)
            }
            if state.busy && state.items.isEmpty {
                Spacer(); ProgressView("Measuring…"); Spacer()
            } else {
                ScrollView {
                    LazyVGrid(columns: columns, alignment: .leading, spacing: 12) {
                        ForEach(state.items) { item in ItemCard(item: item) }
                    }
                }
            }
        }
        .padding(24)
    }
}

struct ItemCard: View {
    @Environment(AppState.self) private var state
    let item: Item
    @State private var expanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top) {
                SymbolIcon(name: item.icon, size: 26)
                VStack(alignment: .leading, spacing: 2) {
                    Text(item.name).font(.headline)
                    Text("\(item.count) · \(humanBytes(item.bytes))").font(.caption).foregroundStyle(.secondary)
                    Text(item.location).font(.caption2.monospaced()).foregroundStyle(.tertiary).lineLimit(1)
                }
                Spacer()
                Toggle("", isOn: Binding(get: { state.isSelected(item) }, set: { _ in state.toggle(item) }))
                    .toggleStyle(.checkbox).labelsHidden()
            }
            DisclosureGroup(isExpanded: $expanded) {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(item.members) { m in MemberRow(item: item, member: m) }
                }
                .padding(.top, 4)
            } label: {
                Text(expanded ? "Hide" : "Show \(item.members.count)").font(.caption).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(selected: state.isSelected(item))
    }
}

struct MemberRow: View {
    @Environment(AppState.self) private var state
    let item: Item
    let member: Member
    var body: some View {
        HStack(spacing: 8) {
            Toggle("", isOn: Binding(get: { state.isSelected(item, member) }, set: { _ in state.toggle(item, member) }))
                .toggleStyle(.checkbox).labelsHidden()
            VStack(alignment: .leading, spacing: 1) {
                Text(member.name).font(.callout).lineLimit(1)
                Text(member.detail).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer()
            HStack(spacing: 3) {
                Image(systemName: "laptopcomputer").foregroundStyle(member.local ? Color.accentColor : Color.secondary.opacity(0.3))
                Image(systemName: "desktopcomputer").foregroundStyle(member.peer ? Color.accentColor : Color.secondary.opacity(0.3))
            }
            .font(.caption)
            .help("Present on this Mac / on the other machine")
            Text(humanBytes(member.bytes)).font(.caption.monospacedDigit()).foregroundStyle(.secondary).frame(width: 64, alignment: .trailing)
            if item.id == "git" {
                Toggle("", isOn: Binding(get: { state.autocommitRepos.contains(member.id) }, set: { _ in state.toggleAutocommit(member) }))
                    .toggleStyle(.switch).controlSize(.mini).labelsHidden()
                    .help("Auto-commit uncommitted work on this repo before syncing")
            }
        }
    }
}
