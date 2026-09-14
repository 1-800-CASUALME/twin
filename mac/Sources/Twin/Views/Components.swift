import SwiftUI

func humanBytes(_ b: Int64) -> String {
    let f = ByteCountFormatter(); f.countStyle = .file; f.allowsNonnumericFormatting = false
    return f.string(fromByteCount: b)
}

struct StatusDot: View {
    let state: StepState
    var body: some View {
        Group {
            switch state {
            case .pending: Image(systemName: "circle").foregroundStyle(.tertiary)
            case .running: ProgressView().controlSize(.mini)
            case .ok: Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
            case .warn: Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
            case .fail: Image(systemName: "xmark.circle.fill").foregroundStyle(.red)
            case .skipped: Image(systemName: "minus.circle").foregroundStyle(.secondary)
            }
        }
        .frame(width: 16, height: 16)
    }
}

/// Large single action button used on Connect, Diagnose and Sync.
struct BigButton: View {
    let title: String
    let symbol: String
    let busy: Bool
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                if busy { ProgressView().controlSize(.small) } else { Image(systemName: symbol) }
                Text(title)
            }
            .font(.title3.weight(.medium))
            .frame(minWidth: 240)
            .padding(.vertical, 6)
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
        .disabled(busy)
    }
}

struct SymbolIcon: View {
    let name: String
    var size: CGFloat = 28
    var pulse = false
    var body: some View {
        Image(systemName: name)
            .font(.system(size: size, weight: .regular))
            .symbolRenderingMode(.hierarchical)
            .foregroundStyle(Color.accentColor)
            .symbolEffect(.pulse, isActive: pulse)
            .frame(width: size + 12, height: size + 12)
    }
}

struct CardBackground: ViewModifier {
    var selected = false
    var disabled = false
    func body(content: Content) -> some View {
        content
            .padding(12)
            .background(.background.secondary, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous)
                .strokeBorder(selected ? Color.accentColor : Color.primary.opacity(0.08), lineWidth: selected ? 2 : 1))
            .opacity(disabled ? 0.45 : 1)
    }
}
extension View { func card(selected: Bool = false, disabled: Bool = false) -> some View { modifier(CardBackground(selected: selected, disabled: disabled)) } }

struct ErrorBanner: View {
    let text: String
    let dismiss: () -> Void
    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
            Text(text).font(.callout).lineLimit(3)
            Spacer()
            Button("", systemImage: "xmark", action: dismiss).buttonStyle(.plain).labelStyle(.iconOnly)
        }
        .padding(10)
        .background(.orange.opacity(0.12), in: RoundedRectangle(cornerRadius: 8))
    }
}
