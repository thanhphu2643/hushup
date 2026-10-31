import Foundation

/// Apple's software-update endpoints. Without them iOS can't find, download or
/// announce a new version, so the Settings badge never comes back.
enum BlockList {
    static let hosts = [
        "mesu.apple.com",             // update catalog
        "gdmf.apple.com",             // which versions this device may install
        "appldnld.apple.com",         // update downloads (older devices)
        "updates.cdn-apple.com",      // update downloads
        "updates-http.cdn-apple.com",
    ]

    static func blocks(_ name: String) -> Bool {
        let name = name.lowercased().trimmingCharacters(in: CharacterSet(charactersIn: "."))
        return hosts.contains { name == $0 || name.hasSuffix("." + $0) }
    }
}

/// Shared between the app and the tunnel extension.
enum TunnelConfig {
    static let bundleID = "io.github.hushup.ios.tunnel"
    /// Fake resolver inside the tunnel; only queries for `BlockList.hosts` are sent here.
    static let dnsAddress = "198.18.0.1"
    static let localAddress = "198.18.0.2"
}

/// An IPv4/UDP datagram read from the tunnel.
struct UDPPacket: Equatable {
    var source: [UInt8]
    var destination: [UInt8]
    var sourcePort: UInt16
    var destinationPort: UInt16
    var payload: [UInt8]

    init(source: [UInt8], destination: [UInt8], sourcePort: UInt16, destinationPort: UInt16, payload: [UInt8]) {
        self.source = source
        self.destination = destination
        self.sourcePort = sourcePort
        self.destinationPort = destinationPort
        self.payload = payload
    }

    init?(_ data: Data) {
        let b = [UInt8](data)
        guard b.count >= 28, b[0] >> 4 == 4, b[9] == 17 else { return nil }
        let ihl = Int(b[0] & 0x0F) * 4
        guard ihl >= 20, b.count >= ihl + 8 else { return nil }
        let udpLength = Int(b[ihl + 4]) << 8 | Int(b[ihl + 5])
        guard udpLength >= 8, ihl + udpLength <= b.count else { return nil }
        source = Array(b[12..<16])
        destination = Array(b[16..<20])
        sourcePort = UInt16(b[ihl]) << 8 | UInt16(b[ihl + 1])
        destinationPort = UInt16(b[ihl + 2]) << 8 | UInt16(b[ihl + 3])
        payload = Array(b[(ihl + 8)..<(ihl + udpLength)])
    }

    /// The datagram as raw bytes (UDP checksum left at 0, which IPv4 allows).
    var bytes: Data {
        let total = 28 + payload.count
        var p = [UInt8](repeating: 0, count: 28)
        p[0] = 0x45
        p[2] = UInt8(total >> 8)
        p[3] = UInt8(total & 0xFF)
        p[6] = 0x40 // don't fragment
        p[8] = 64   // TTL
        p[9] = 17   // UDP
        p.replaceSubrange(12..<16, with: source)
        p.replaceSubrange(16..<20, with: destination)
        let sum = checksum(p[0..<20])
        p[10] = UInt8(sum >> 8)
        p[11] = UInt8(sum & 0xFF)
        p[20] = UInt8(sourcePort >> 8)
        p[21] = UInt8(sourcePort & 0xFF)
        p[22] = UInt8(destinationPort >> 8)
        p[23] = UInt8(destinationPort & 0xFF)
        p[24] = UInt8((8 + payload.count) >> 8)
        p[25] = UInt8((8 + payload.count) & 0xFF)
        return Data(p + payload)
    }

    /// Same addresses and ports, reversed, carrying `payload`.
    func reply(_ payload: [UInt8]) -> UDPPacket {
        UDPPacket(source: destination, destination: source, sourcePort: destinationPort,
                  destinationPort: sourcePort, payload: payload)
    }
}

private func checksum(_ header: ArraySlice<UInt8>) -> UInt16 {
    var sum: UInt32 = 0
    var i = header.startIndex
    while i < header.endIndex {
        sum += UInt32(header[i]) << 8 | UInt32(header[i + 1])
        i += 2
    }
    while sum > 0xFFFF { sum = (sum & 0xFFFF) + (sum >> 16) }
    return ~UInt16(sum)
}

enum DNS {
    static let nxdomain: UInt8 = 3
    static let refused: UInt8 = 5

    /// The first question of a query: its name (e.g. "mesu.apple.com") and where the question ends.
    static func question(_ m: [UInt8]) -> (name: String, end: Int)? {
        guard m.count > 12, m[2] & 0x80 == 0, Int(m[4]) << 8 | Int(m[5]) >= 1 else { return nil }
        var labels: [String] = []
        var i = 12
        while i < m.count {
            let length = Int(m[i])
            if length == 0 {
                let end = i + 5 // terminator + QTYPE + QCLASS
                return end <= m.count ? (labels.joined(separator: "."), end) : nil
            }
            guard length < 64, i + 1 + length <= m.count else { return nil } // no compression in questions
            labels.append(String(decoding: m[(i + 1)...(i + length)], as: UTF8.self))
            i += 1 + length
        }
        return nil
    }

    /// An empty answer with `rcode`: same id and question, no records.
    static func answer(_ query: [UInt8], rcode: UInt8) -> [UInt8]? {
        guard let q = question(query) else { return nil }
        var r = Array(query[0..<q.end])
        r[2] = 0x80 | (query[2] & 0x79) // response, keep opcode and RD
        r[3] = 0x80 | rcode             // recursion available
        r.replaceSubrange(4..<12, with: [0, 1, 0, 0, 0, 0, 0, 0])
        return r
    }

    /// What the tunnel says to a datagram: "no such domain" for update servers.
    static func respond(to data: Data) -> Data? {
        guard let packet = UDPPacket(data), packet.destinationPort == 53,
              let q = question(packet.payload),
              let answer = answer(packet.payload, rcode: BlockList.blocks(q.name) ? nxdomain : refused)
        else { return nil }
        return packet.reply(answer).bytes
    }
}
