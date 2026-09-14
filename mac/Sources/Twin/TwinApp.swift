import SwiftUI

@main
struct TwinApp: App {
    @State private var state = AppState()

    var body: some Scene {
        WindowGroup("Twin") {
            RootView()
                .environment(state)
                .frame(width: 760, height: 500)
                .task { state.onLaunch() }
        }
        .windowResizability(.contentSize)
        .windowStyle(.hiddenTitleBar)
        .commands { CommandGroup(replacing: .newItem) {} }
    }
}
