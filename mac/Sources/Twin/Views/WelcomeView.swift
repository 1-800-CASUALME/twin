import SwiftUI

struct WelcomeView: View {
    var body: some View {
        VStack(spacing: 18) {
            Spacer()
            SymbolIcon(name: "arrow.triangle.2.circlepath", size: 72, pulse: true)
            Text("Twin").font(.largeTitle.weight(.bold))
            Text("Two machines. One workspace.").font(.title3).foregroundStyle(.secondary)
            HStack(spacing: 28) {
                miniStep("link", "Connect")
                miniStep("stethoscope", "Diagnose")
                miniStep("arrow.triangle.2.circlepath", "Sync")
            }
            .padding(.top, 12)
            Spacer()
        }
        .padding(30)
    }
    private func miniStep(_ symbol: String, _ title: String) -> some View {
        VStack(spacing: 6) {
            Image(systemName: symbol).font(.title2).foregroundStyle(Color.accentColor)
            Text(title).font(.caption).foregroundStyle(.secondary)
        }
    }
}
