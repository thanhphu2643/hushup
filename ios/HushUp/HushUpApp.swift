import NetworkExtension
import SwiftUI

/// iOS front end. iOS gives apps no switch for system updates, so the blocking is a
/// local DNS-only VPN (see HushUpTunnel) that hides Apple's update servers.
@main
struct HushUpApp: App {
    @StateObject private var model = Model()
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            MainView(model: model)
                .onChange(of: phase) { if $0 == .active { Task { await model.refresh() } } }
        }
    }
}

struct MainView: View {
    @ObservedObject var model: Model

    var body: some View {
        ScrollView {
            VStack(spacing: 20) {
                Image("Icon")
                    .resizable()
                    .frame(width: 96, height: 96)
                    .clipShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
                    .padding(.top, 24)
                VStack(spacing: 4) {
                    Text(model.isBlocked ? "Updates are blocked" : "Updates are allowed")
                        .font(.title2.weight(.semibold))
                    Text(model.isBlocked
                         ? "iOS can't find, download or nag you about new versions."
                         : "iOS checks for updates and shows the red badge as usual.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                }

#if NO_TUNNEL
                dnsProfileSteps
#else
                tunnelControls
#endif
            }
            .padding(24)
            .frame(maxWidth: 480)
            .frame(maxWidth: .infinity)
        }
        .task { await model.refresh() }
    }

    /// Paid developer accounts: the on-device DNS tunnel.
    @ViewBuilder private var tunnelControls: some View {
        card {
            row("Apple update servers blocked", ok: model.serversBlocked)
            Divider()
            row("Stays on after restart", ok: model.isBlocked && model.onDemand)
        }

        VStack(alignment: .leading, spacing: 10) {
            Text("Do once in Settings › General")
                .font(.footnote.weight(.semibold))
                .foregroundStyle(.secondary)
            step(1, "Software Update › Automatic Updates: turn everything off.")
            step(2, "iPhone Storage › the iOS update › Delete Update, if one was already downloaded. That clears the badge.")
        }
        .frame(maxWidth: .infinity, alignment: .leading)

        if let error = model.error {
            Text(error)
                .font(.callout)
                .foregroundStyle(.red)
                .multilineTextAlignment(.center)
        }

        Group {
            if model.isBlocked {
                Button { Task { await model.restore() } } label: {
                    Text("Restore Updates").frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
            } else {
                Button { Task { await model.block() } } label: {
                    Text("Block Updates").frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
            }
        }
        .controlSize(.large)
        .disabled(model.busy)

        Text("Shows up as a VPN named HushUp. Nothing leaves your iPhone: only lookups of Apple's update servers are answered with \"not found\".")
            .font(.footnote)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
    }

    /// Free Apple IDs can't sign Network Extensions, so the same block goes
    /// through a NextDNS profile; the app checks that it works.
    @ViewBuilder private var dnsProfileSteps: some View {
        card {
            row("Apple update servers blocked", ok: model.serversBlocked)
        }

        VStack(alignment: .leading, spacing: 10) {
            Text("Block with a free NextDNS profile")
                .font(.footnote.weight(.semibold))
                .foregroundStyle(.secondary)
            step(1, "Sign up at my.nextdns.io. In Denylist, add the update servers (Copy Server List below).")
            step(2, "Open apple.nextdns.io in Safari, enter your configuration ID and download the profile.")
            step(3, "Settings › Profile Downloaded › Install. Then General › VPN & Device Management › DNS: pick NextDNS.")
            Text("Then once in Settings › General")
                .font(.footnote.weight(.semibold))
                .foregroundStyle(.secondary)
                .padding(.top, 6)
            step(4, "Software Update › Automatic Updates: turn everything off.")
            step(5, "iPhone Storage › the iOS update › Delete Update, if one was already downloaded. That clears the badge.")
        }
        .frame(maxWidth: .infinity, alignment: .leading)

        VStack(spacing: 10) {
            Button {
                UIPasteboard.general.string = BlockList.hosts.joined(separator: "\n")
                model.copied = true
            } label: {
                Text(model.copied ? "Copied" : "Copy Server List").frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            HStack(spacing: 10) {
                Link(destination: URL(string: "https://my.nextdns.io")!) {
                    Text("Open NextDNS").frame(maxWidth: .infinity)
                }
                Link(destination: URL(string: "https://apple.nextdns.io")!) {
                    Text("Get Profile").frame(maxWidth: .infinity)
                }
            }
            .buttonStyle(.bordered)
            Button { Task { await model.refresh() } } label: {
                Text("Check Again").frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
        }
        .controlSize(.large)

        Text("Undo: delete the NextDNS profile in VPN & Device Management.")
            .font(.footnote)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
    }

    private func card<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        VStack(spacing: 0, content: content)
            .padding(.horizontal, 14)
            .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 12))
    }

    private func row(_ title: String, ok: Bool) -> some View {
        HStack {
            Text(title)
            Spacer()
            Image(systemName: ok ? "checkmark.circle.fill" : "xmark.circle.fill")
                .foregroundStyle(ok ? .green : .secondary)
        }
        .padding(.vertical, 11)
    }

    private func step(_ n: Int, _ text: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Text("\(n)")
                .font(.caption.weight(.bold))
                .foregroundStyle(.white)
                .frame(width: 20, height: 20)
                .background(Color.accentColor, in: Circle())
            Text(text).font(.callout)
        }
    }
}

@MainActor
final class Model: ObservableObject {
    @Published private(set) var manager: NETunnelProviderManager?
    @Published private(set) var serversBlocked = false
    @Published private(set) var busy = false
    @Published var error: String?
    @Published var copied = false

    var onDemand: Bool { manager?.isOnDemandEnabled == true }
#if NO_TUNNEL
    var isBlocked: Bool { serversBlocked }
#else
    var isBlocked: Bool { manager?.isEnabled == true && onDemand }
#endif

    init() {
        NotificationCenter.default.addObserver(forName: .NEVPNStatusDidChange, object: nil, queue: .main) { [weak self] _ in
            Task { await self?.refresh() }
        }
    }

    func refresh() async {
#if !NO_TUNNEL
        manager = try? await NETunnelProviderManager.loadAllFromPreferences().first
#endif
        serversBlocked = await !Self.resolves(BlockList.hosts[0])
    }

    /// Installs the VPN profile (iOS asks once) and keeps it connected on demand.
    func block() async {
        await run {
            let m = manager ?? NETunnelProviderManager()
            let proto = NETunnelProviderProtocol()
            proto.providerBundleIdentifier = TunnelConfig.bundleID
            proto.serverAddress = "On-device"
            m.protocolConfiguration = proto
            m.localizedDescription = "HushUp"
            m.onDemandRules = [NEOnDemandRuleConnect()]
            m.isOnDemandEnabled = true
            m.isEnabled = true
            try await m.saveToPreferences()
            try await m.loadFromPreferences() // required before starting a freshly saved profile
            try m.connection.startVPNTunnel()
        }
    }

    /// Removes the VPN profile entirely, leaving nothing behind.
    func restore() async {
        await run { try await manager?.removeFromPreferences() }
    }

    private func run(_ action: () async throws -> Void) async {
        busy = true
        error = nil
        do {
            try await action()
        } catch {
            self.error = error.localizedDescription
        }
        // Give the tunnel a moment to come up or go down before probing DNS.
        try? await Task.sleep(nanoseconds: 1_500_000_000)
        await refresh()
        busy = false
    }

    private nonisolated static func resolves(_ host: String) async -> Bool {
        await Task.detached {
            var result: UnsafeMutablePointer<addrinfo>?
            let ok = getaddrinfo(host, nil, nil, &result) == 0
            if let result { freeaddrinfo(result) }
            return ok
        }.value
    }
}
