import SwiftUI

final class AppDelegate: NSObject, NSApplicationDelegate {
    var onTerminate: (() -> Void)?
    func applicationWillTerminate(_ notification: Notification) { onTerminate?() }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}

@main
struct TwinApp: App {
    @State private var state = AppState()
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        WindowGroup("Twin") {
            RootView()
                .environment(state)
                .frame(width: 760, height: 500)
                .task { state.onLaunch(); delegate.onTerminate = { [state] in state.shutdown() } }
        }
        .windowResizability(.contentSize)
        .windowStyle(.hiddenTitleBar)
        .commands { CommandGroup(replacing: .newItem) {} }
    }
}
