import Foundation

/// Handle to a running `twin` process. Lets the UI answer prompts (pairing code) and stop it.
final class ProcessHandle: @unchecked Sendable {
    fileprivate let process: Process
    fileprivate let stdin: Pipe
    init(process: Process, stdin: Pipe) { self.process = process; self.stdin = stdin }
    func write(_ line: String) {
        if let d = (line + "\n").data(using: .utf8) { stdin.fileHandleForWriting.write(d) }
    }
    func terminate() { if process.isRunning { process.terminate() } }
    var isRunning: Bool { process.isRunning }
}

/// Accumulates pipe output and hands back complete lines. Locked because the pipe
/// readability handler runs on a background queue.
final class LineBuffer: @unchecked Sendable {
    private var data = Data()
    private let lock = NSLock()
    func append(_ d: Data) -> [Data] {
        lock.lock(); defer { lock.unlock() }
        data.append(d)
        var lines: [Data] = []
        while let nl = data.firstIndex(of: 0x0A) {
            let line = data[data.startIndex..<nl]
            if !line.isEmpty { lines.append(Data(line)) }
            data.removeSubrange(data.startIndex...nl)
        }
        return lines
    }
}

enum TwinCore {
    /// Where the `twin` CLI lives: next to the app binary inside the bundle, or a user install.
    static func binaryURL() -> URL {
        let candidates: [URL] = [
            Bundle.main.executableURL?.deletingLastPathComponent().appendingPathComponent("twin-cli"),
            FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".local/bin/twin"),
            URL(fileURLWithPath: "/opt/homebrew/bin/twin"),
            URL(fileURLWithPath: "/usr/local/bin/twin"),
        ].compactMap { $0 }
        for c in candidates where FileManager.default.isExecutableFile(atPath: c.path) { return c }
        return candidates[0]
    }

    /// Make `twin` reachable from SSH login shells on this Mac by linking it into ~/.local/bin.
    static func ensureUserSymlink() {
        let bin = binaryURL()
        let dir = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".local/bin")
        let link = dir.appendingPathComponent("twin")
        guard bin.path != link.path, FileManager.default.isExecutableFile(atPath: bin.path) else { return }
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        if let existing = try? FileManager.default.destinationOfSymbolicLink(atPath: link.path), existing == bin.path { return }
        try? FileManager.default.removeItem(at: link)
        try? FileManager.default.createSymbolicLink(at: link, withDestinationURL: bin)
    }

    /// Spawn `twin <args>` and stream its NDJSON events.
    static func run(_ args: [String]) -> (AsyncStream<Event>, ProcessHandle) {
        let p = Process()
        p.executableURL = binaryURL()
        p.arguments = args
        var env = ProcessInfo.processInfo.environment
        env["PATH"] = "/opt/homebrew/bin:/usr/local/bin:" + (env["PATH"] ?? "/usr/bin:/bin")
        p.environment = env
        let out = Pipe(), err = Pipe(), inp = Pipe()
        p.standardOutput = out; p.standardError = err; p.standardInput = inp
        let handle = ProcessHandle(process: p, stdin: inp)
        let stream = AsyncStream<Event> { cont in
            let buffer = LineBuffer()
            out.fileHandleForReading.readabilityHandler = { fh in
                let d = fh.availableData
                if d.isEmpty { return }
                for line in buffer.append(d) {
                    if let ev = try? JSONDecoder().decode(Event.self, from: line) { cont.yield(ev) }
                    else { NSLog("twin: unparsed line: %@", String(decoding: line, as: UTF8.self)) }
                }
            }
            err.fileHandleForReading.readabilityHandler = { fh in
                let d = fh.availableData
                if !d.isEmpty { NSLog("twin stderr: %@", String(decoding: d, as: UTF8.self)) }
            }
            p.terminationHandler = { _ in
                out.fileHandleForReading.readabilityHandler = nil
                err.fileHandleForReading.readabilityHandler = nil
                cont.finish()
            }
            do { try p.run() } catch {
                cont.yield(.error("could not start twin at \(binaryURL().path): \(error.localizedDescription)"))
                cont.yield(.done(ok: false))
                cont.finish()
            }
        }
        return (stream, handle)
    }
}
