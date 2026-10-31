import NetworkExtension

/// A DNS-only "VPN" that never leaves the device. Split DNS sends just the
/// update-server lookups to a resolver inside the tunnel, which answers
/// "no such domain"; every other lookup and all traffic use the normal network.
final class PacketTunnelProvider: NEPacketTunnelProvider {
    override func startTunnel(options: [String: NSObject]?) async throws {
        let settings = NEPacketTunnelNetworkSettings(tunnelRemoteAddress: "127.0.0.1")
        let ipv4 = NEIPv4Settings(addresses: [TunnelConfig.localAddress], subnetMasks: ["255.255.255.255"])
        ipv4.includedRoutes = [NEIPv4Route(destinationAddress: TunnelConfig.dnsAddress, subnetMask: "255.255.255.255")]
        settings.ipv4Settings = ipv4
        let dns = NEDNSSettings(servers: [TunnelConfig.dnsAddress])
        dns.matchDomains = BlockList.hosts
        dns.matchDomainsNoSearch = true
        settings.dnsSettings = dns
        try await setTunnelNetworkSettings(settings)
        readPackets()
    }

    override func stopTunnel(with reason: NEProviderStopReason) async {}

    private func readPackets() {
        packetFlow.readPackets { [weak self] packets, _ in
            guard let self else { return }
            let replies = packets.compactMap(DNS.respond)
            if !replies.isEmpty {
                packetFlow.writePackets(replies, withProtocols: replies.map { _ in AF_INET as NSNumber })
            }
            readPackets()
        }
    }
}
