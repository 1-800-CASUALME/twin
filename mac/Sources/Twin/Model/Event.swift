import Foundation

enum StepState: String, Decodable, Sendable {
    case pending, running, ok, warn, fail, skipped
}

struct Member: Decodable, Identifiable, Hashable, Sendable {
    var id: String
    var name: String
    var path: String
    var bytes: Int64
    var count: Int64
    var detail: String
    var local: Bool
    var peer: Bool
}

struct Item: Decodable, Identifiable, Hashable, Sendable {
    var id: String
    var name: String
    var icon: String
    var bytes: Int64
    var count: Int64
    var location: String
    var members: [Member]
}

struct CheckResult: Decodable, Identifiable, Hashable, Sendable {
    var id: String
    var name: String
    var side: String
    var state: StepState
    var msg: String
    var fixable: Bool
    var key: String { "\(side):\(id)" }
}

struct PeerInfo: Decodable, Identifiable, Hashable, Sendable {
    var name: String
    var host: String
    var os: String
    var addr: String
    var port: Int
    var instance: String
    var id: String { instance }
}

struct Conflict: Identifiable, Hashable, Sendable {
    var id = UUID()
    var item: String
    var path: String
    var kept: String
}

/// One NDJSON line from the `twin` CLI. Field names mirror the Rust `Event` enum.
enum Event: Decodable, Sendable {
    case step(id: String, state: StepState, msg: String)
    case progress(id: String, done: Int64, total: Int64, bytes: Int64)
    case conflict(id: String, path: String, kept: String)
    case peer(PeerInfo)
    case code(String)
    case item(Item)
    case check(CheckResult)
    case done(ok: Bool)
    case error(String)

    private enum K: String, CodingKey { case ev, id, state, msg, done, total, bytes, path, kept, code, ok }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: K.self)
        switch try c.decode(String.self, forKey: .ev) {
        case "step":
            self = .step(id: try c.decode(String.self, forKey: .id), state: try c.decode(StepState.self, forKey: .state), msg: try c.decode(String.self, forKey: .msg))
        case "progress":
            self = .progress(id: try c.decode(String.self, forKey: .id), done: try c.decode(Int64.self, forKey: .done), total: try c.decode(Int64.self, forKey: .total), bytes: try c.decode(Int64.self, forKey: .bytes))
        case "conflict":
            self = .conflict(id: try c.decode(String.self, forKey: .id), path: try c.decode(String.self, forKey: .path), kept: try c.decode(String.self, forKey: .kept))
        case "peer":
            self = .peer(try PeerInfo(from: decoder))
        case "code":
            self = .code(try c.decode(String.self, forKey: .code))
        case "item":
            self = .item(try Item(from: decoder))
        case "check":
            self = .check(try CheckResult(from: decoder))
        case "done":
            self = .done(ok: try c.decode(Bool.self, forKey: .ok))
        case "error":
            self = .error(try c.decode(String.self, forKey: .msg))
        case let other:
            throw DecodingError.dataCorruptedError(forKey: .ev, in: c, debugDescription: "unknown event \(other)")
        }
    }
}
