import SwiftUI

struct ConnectView: View {
    @Environment(AppState.self) private var state

    var body: some View {
        VStack(spacing: 20) {
            Spacer(minLength: 8)
            if let p = state.paired {
                SymbolIcon(name: "checkmark.circle.fill", size: 64)
                    .foregroundStyle(.green)
                Text("Connected to \(p.name)").font(.title2.weight(.semibold))
                if !p.addr.isEmpty { Text(p.addr).font(.callout.monospaced()).foregroundStyle(.secondary) }
                Button("Pair a different machine") { state.paired = nil }.buttonStyle(.link).font(.callout)
            } else {
                SymbolIcon(name: "wifi", size: 56, pulse: state.busy)
                Text("Find the other machine").font(.title2.weight(.semibold))
                Text("Open Twin on both machines on the same network.").font(.callout).foregroundStyle(.secondary)
                BigButton(title: state.busy ? "Looking…" : "Find", symbol: "magnifyingglass", busy: state.busy) {
                    Task { await state.discover() }
                }
                if !state.peers.isEmpty {
                    VStack(spacing: 8) {
                        ForEach(state.peers) { peer in
                            Button { state.pair(peer) } label: {
                                HStack(spacing: 12) {
                                    Image(systemName: peer.os == "linux" ? "desktopcomputer" : "laptopcomputer")
                                        .font(.title2).foregroundStyle(Color.accentColor).frame(width: 32)
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(peer.name).font(.headline)
                                        Text("\(peer.os.capitalized) · \(peer.addr)").font(.caption).foregroundStyle(.secondary)
                                    }
                                    Spacer()
                                    if state.pairingPeer?.instance == peer.instance { ProgressView().controlSize(.small) }
                                    else { Image(systemName: "chevron.right").foregroundStyle(.tertiary) }
                                }
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .card()
                        }
                    }
                    .frame(maxWidth: 380)
                } else if !state.busy {
                    Text("Nothing found yet.").font(.caption).foregroundStyle(.tertiary)
                }
            }
            Spacer()
            Link(destination: URL(string: "https://tailscale.com/kb/1017/install")!) {
                Label("Away from home? Use Tailscale on both machines.", systemImage: "globe").font(.caption)
            }
        }
        .padding(30)
    }
}
