import SwiftUI

struct RootView: View {
    @Environment(AppState.self) private var state

    var body: some View {
        HStack(spacing: 0) {
            sidebar
            Divider()
            VStack(spacing: 0) {
                content.frame(maxWidth: .infinity, maxHeight: .infinity)
                Divider()
                bottomBar
            }
        }
        .sheet(isPresented: Binding(get: { state.pairingCode != nil }, set: { if !$0 { state.confirmCode(false) } })) {
            CodeSheet()
        }
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Image(systemName: "arrow.triangle.2.circlepath").font(.title2).foregroundStyle(Color.accentColor)
                Text("Twin").font(.title2.weight(.semibold))
            }
            .padding(.bottom, 18)
            ForEach(Step.allCases) { s in
                HStack(spacing: 10) {
                    Image(systemName: s.rawValue < state.step.rawValue ? "checkmark.circle.fill" : s.symbol)
                        .foregroundStyle(s.rawValue < state.step.rawValue ? .green : (s == state.step ? Color.accentColor : .secondary))
                        .frame(width: 18)
                    Text(s.title)
                        .foregroundStyle(s == state.step ? .primary : .secondary)
                        .fontWeight(s == state.step ? .semibold : .regular)
                }
                .padding(.vertical, 6).padding(.horizontal, 8)
                .background(s == state.step ? Color.accentColor.opacity(0.12) : .clear, in: RoundedRectangle(cornerRadius: 6))
            }
            Spacer()
            if let p = state.paired {
                Label(p.name, systemImage: "link").font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
        }
        .padding(20)
        .frame(width: 200, alignment: .topLeading)
        .background(.ultraThinMaterial)
    }

    @ViewBuilder private var content: some View {
        switch state.step {
        case .welcome: WelcomeView()
        case .connect: ConnectView()
        case .diagnose: DiagnoseView()
        case .choose: ChooseView()
        case .sync: SyncView()
        case .done: DoneView()
        }
    }

    private var bottomBar: some View {
        HStack {
            if let e = state.error { ErrorBanner(text: e) { state.error = nil }.frame(maxWidth: 420) }
            Spacer()
            if state.step != .welcome && state.step != .done {
                Button("Back") { state.back() }.disabled(state.busy)
            }
            if state.step != .done {
                Button("Continue") { state.next() }
                    .keyboardShortcut(.defaultAction)
                    .buttonStyle(.borderedProminent)
                    .disabled(!state.canContinue || state.busy)
            } else {
                Button("Sync Again") { state.restart() }.buttonStyle(.borderedProminent)
            }
        }
        .padding(14)
    }
}

struct CodeSheet: View {
    @Environment(AppState.self) private var state
    var body: some View {
        VStack(spacing: 18) {
            SymbolIcon(name: "lock.shield", size: 40, pulse: true)
            Text("Same code on both screens?").font(.headline)
            Text(state.pairingCode ?? "")
                .font(.system(size: 44, weight: .semibold, design: .rounded).monospacedDigit())
                .kerning(6)
            Text("Confirm on both machines to pair.").font(.callout).foregroundStyle(.secondary)
            HStack {
                Button("Cancel") { state.confirmCode(false) }.keyboardShortcut(.cancelAction)
                Button("Confirm") { state.confirmCode(true) }.keyboardShortcut(.defaultAction).buttonStyle(.borderedProminent)
            }
        }
        .padding(28)
        .frame(width: 360)
    }
}
