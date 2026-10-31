import XCTest

final class DNSTests: XCTestCase {
    /// A query for `name` (type A) from 198.18.0.2:5353 to the tunnel resolver.
    private func query(_ name: String, id: [UInt8] = [0xAB, 0xCD]) -> Data {
        var dns = id + [0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]
        for label in name.split(separator: ".") {
            dns += [UInt8(label.utf8.count)] + Array(label.utf8)
        }
        dns += [0, 0, 1, 0, 1]
        return UDPPacket(source: [198, 18, 0, 2], destination: [198, 18, 0, 1],
                         sourcePort: 5353, destinationPort: 53, payload: dns).bytes
    }

    func testBlocksUpdateHostsAndSubdomains() {
        XCTAssertTrue(BlockList.blocks("mesu.apple.com"))
        XCTAssertTrue(BlockList.blocks("MESU.apple.com."))
        XCTAssertTrue(BlockList.blocks("a.gdmf.apple.com"))
        XCTAssertFalse(BlockList.blocks("apple.com"))
        XCTAssertFalse(BlockList.blocks("notmesu.apple.com"))
    }

    func testPacketRoundTrips() throws {
        let data = query("mesu.apple.com")
        let packet = try XCTUnwrap(UDPPacket(data))
        XCTAssertEqual(packet.bytes, data)
        XCTAssertEqual(packet.destinationPort, 53)
        XCTAssertEqual(DNS.question(packet.payload)?.name, "mesu.apple.com")
    }

    func testIPv4HeaderChecksumIsValid() {
        let header = [UInt8](query("mesu.apple.com").prefix(20))
        var sum: UInt32 = 0
        for i in stride(from: 0, to: 20, by: 2) { sum += UInt32(header[i]) << 8 | UInt32(header[i + 1]) }
        while sum > 0xFFFF { sum = (sum & 0xFFFF) + (sum >> 16) }
        XCTAssertEqual(sum, 0xFFFF)
    }

    func testAnswersUpdateHostWithNXDOMAIN() throws {
        let reply = try XCTUnwrap(UDPPacket(XCTUnwrap(DNS.respond(to: query("gdmf.apple.com")))))
        XCTAssertEqual(reply.source, [198, 18, 0, 1])
        XCTAssertEqual(reply.destination, [198, 18, 0, 2])
        XCTAssertEqual(reply.sourcePort, 53)
        XCTAssertEqual(reply.destinationPort, 5353)
        let dns = reply.payload
        XCTAssertEqual(Array(dns[0..<2]), [0xAB, 0xCD]) // same id
        XCTAssertEqual(dns[2] & 0x80, 0x80)             // a response
        XCTAssertEqual(dns[2] & 0x01, 0x01)             // RD kept
        XCTAssertEqual(dns[3] & 0x0F, DNS.nxdomain)
        XCTAssertEqual(Array(dns[4..<12]), [0, 1, 0, 0, 0, 0, 0, 0])
        XCTAssertEqual(DNS.question(Array(query("gdmf.apple.com").dropFirst(28)))?.end, dns.count)
    }

    func testRefusesAnythingElse() throws {
        let reply = try XCTUnwrap(UDPPacket(XCTUnwrap(DNS.respond(to: query("www.apple.com")))))
        XCTAssertEqual(reply.payload[3] & 0x0F, DNS.refused)
    }

    func testIgnoresGarbage() {
        XCTAssertNil(DNS.respond(to: Data([0x45, 0, 0, 20])))
        XCTAssertNil(DNS.respond(to: Data(repeating: 0, count: 40)))
        var truncated = query("mesu.apple.com")
        truncated.removeLast(3)
        XCTAssertNil(DNS.respond(to: truncated)) // UDP length now runs past the packet
    }
}
