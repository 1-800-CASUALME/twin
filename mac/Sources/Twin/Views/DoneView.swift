import SwiftUI

struct DoneView: View {
    @Environment(AppState.self) private var state

    var body: some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: state.syncOK ? "checkmark.seal.fill" : "exclamationmark.triangle.fill")
                .font(.system(size: 72)).symbolRenderingMode(.hierarchical)
                .foregroundStyle(state.syncOK ? .green : .orange)
            Text(state.syncOK ? "In sync" : "Finished with issues").font(.largeTitle.weight(.bold))
            VStack(alignment: .leading, spacing: 4) {
                ForEach(state.summaryLines, id: \.self) { Text($0).font(.callout) }
            }
            .foregroundStyle(.secondary)
            Toggle("Keep in sync every 15 minutes", isOn: Binding(get: { state.schedule }, set: { on in Task { await state.setSchedule(on) } }))
                .toggleStyle(.switch)
            Text("Attach to the other machine from Terminal with  twin attach")
                .font(.caption).foregroundStyle(.tertiary)
            Spacer()
        }
        .padding(30)
    }
}
