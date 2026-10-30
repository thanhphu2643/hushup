import AppKit
import ServiceManagement
import SwiftUI

/// Menu bar front end. All real work is done by the `hushup` Rust CLI.
@main
struct HushUpApp: App {
    @NSApplicationDelegateAdaptor private var delegate: AppDelegate

    var body: some Scene {
        MenuBarExtra {
            MenuContent(model: delegate.model, openWindow: delegate.showWindow)
        } label: {
            MenuBarLabel(model: delegate.model)
        }
    }
}

/// Owns the main window. It's plain AppKit so a double-click in Finder on the
/// already-running app (a "reopen") can always bring it back.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let model = Model()
    private var window: NSWindow?

    func applicationDidFinishLaunching(_ notification: Notification) {
        if !launchedAsLoginItem() {
            showWindow()
        }
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows: Bool) -> Bool {
        showWindow()
        return false
    }

    func showWindow() {
        if window == nil {
            let w = NSWindow(contentViewController: NSHostingController(rootView: MainView(model: model)))
            w.title = "HushUp"
            w.styleMask = [.titled, .closable, .miniaturizable, .fullSizeContentView]
            w.titlebarAppearsTransparent = true
            w.isReleasedWhenClosed = false
            w.center()
            window = w
        }
        model.refresh()
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }

    private func launchedAsLoginItem() -> Bool {
        let event = NSAppleEventManager.shared().currentAppleEvent
        return event?.eventID == kAEOpenApplication
            && event?.paramDescriptor(forKeyword: keyAEPropData)?.enumCodeValue == keyAELaunchedAsLogInItem
    }
}

struct MenuBarLabel: View {
    @ObservedObject var model: Model

    var body: some View {
        Image(nsImage: menuBarIcon(model.status?.applied == true ? "menubar-on" : "menubar-off"))
    }
}

/// Bundled template image (see scripts/make-icons.swift); macOS tints it for light/dark menu bars.
private func menuBarIcon(_ name: String) -> NSImage {
    let image = NSImage(named: name) ?? NSImage(systemSymbolName: "bell.slash", accessibilityDescription: "HushUp")!
    image.isTemplate = true
    return image
}

struct MenuContent: View {
    @ObservedObject var model: Model
    let openWindow: () -> Void

    var body: some View {
        if let s = model.status {
            Text(s.applied ? "Updates blocked" : "Updates allowed")
            Text("Auto check: \(s.autoCheck ? "on" : "off") · Hosts blocked: \(s.hostsBlocked ? "yes" : "no")")
            Text("Background guard: \(s.daemonInstalled ? "running" : "off")")
            ForEach(s.pendingUpdates, id: \.self) { Text("Pending: \($0)") }
        }
        if let error = model.error {
            Text(error)
        }
        Divider()
        Button("Open HushUp…") { openWindow() }
        Button("Block Updates") { model.block() }
            .disabled(model.isBlocked)
        Button("Restore Updates") { model.restore() }
            .disabled(model.status?.applied != true)
        Button("Refresh") { model.refresh() }
        Divider()
        Toggle("Launch at Login", isOn: Binding(get: { model.launchAtLogin }, set: { model.setLaunchAtLogin($0) }))
        Button("Quit HushUp") { NSApp.terminate(nil) }
    }
}

struct MainView: View {
    @ObservedObject var model: Model

    var body: some View {
        VStack(spacing: 20) {
            Image(nsImage: NSApp.applicationIconImage)
                .resizable()
                .frame(width: 96, height: 96)
            VStack(spacing: 4) {
                Text(model.isBlocked ? "Updates are blocked" : "Updates are allowed")
                    .font(.title2.weight(.semibold))
                Text(model.isBlocked
                     ? "macOS won't check for, download or nag you about updates."
                     : "macOS checks for updates and shows the red badge as usual.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }

            if let s = model.status {
                VStack(spacing: 0) {
                    row("Automatic update check", ok: !s.autoCheck)
                    Divider()
                    row("Automatic download & install", ok: !s.autoDownload && !s.autoInstallOs)
                    Divider()
                    row("Apple update servers blocked", ok: s.hostsBlocked)
                    Divider()
                    row("Background guard", ok: s.daemonInstalled)
                    Divider()
                    HStack {
                        Text("Pending updates")
                        Spacer()
                        Text(s.pendingUpdates.isEmpty ? "None" : s.pendingUpdates.joined(separator: ", "))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    .padding(.vertical, 8)
                }
                .padding(.horizontal, 14)
                .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 10))
            }

            if let error = model.error {
                Text(error)
                    .font(.callout)
                    .foregroundStyle(.red)
            }

            HStack {
                Toggle("Launch at Login", isOn: Binding(get: { model.launchAtLogin }, set: { model.setLaunchAtLogin($0) }))
                    .toggleStyle(.switch)
                    .controlSize(.small)
                Spacer()
                if model.isBlocked {
                    Button("Restore Updates") { model.restore() }
                        .controlSize(.large)
                } else {
                    Button("Block Updates") { model.block() }
                        .controlSize(.large)
                        .buttonStyle(.borderedProminent)
                }
            }
        }
        .padding(24)
        .padding(.top, 8)
        .frame(width: 420)
    }

    private func row(_ title: String, ok: Bool) -> some View {
        HStack {
            Text(title)
            Spacer()
            Image(systemName: ok ? "checkmark.circle.fill" : "xmark.circle.fill")
                .foregroundStyle(ok ? .green : .secondary)
        }
        .padding(.vertical, 8)
    }
}

struct Status: Decodable {
    let applied: Bool
    let autoCheck: Bool
    let autoDownload: Bool
    let autoInstallOs: Bool
    let pendingUpdates: [String]
    let notificationCount: Int
    let hostsBlocked: Bool
    let daemonInstalled: Bool
}

@MainActor
final class Model: ObservableObject {
    @Published var status: Status?
    @Published var error: String?
    @Published var launchAtLogin = SMAppService.mainApp.status == .enabled

    private let cli: String? = Model.findCLI()

    var isBlocked: Bool { status?.applied == true && status?.daemonInstalled == true }

    init() {
        refresh()
    }

    func refresh() {
        guard let cli else {
            error = "hushup CLI not found"
            return
        }
        do {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            status = try decoder.decode(Status.self, from: try run(cli, ["status", "--json"]))
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }

    func block() {
        guard let cli else { return }
        runAsAdmin("\(shellQuote(cli)) apply && \(shellQuote(cli)) install-daemon")
    }

    func restore() {
        guard let cli else { return }
        runAsAdmin("\(shellQuote(cli)) uninstall-daemon; \(shellQuote(cli)) restore")
    }

    func setLaunchAtLogin(_ on: Bool) {
        do {
            try on ? SMAppService.mainApp.register() : SMAppService.mainApp.unregister()
        } catch {
            self.error = error.localizedDescription
        }
        launchAtLogin = SMAppService.mainApp.status == .enabled
    }

    /// Shows the standard macOS password prompt. Good enough until we ship an SMAppService helper.
    private func runAsAdmin(_ command: String) {
        let escaped = command.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: "\"", with: "\\\"")
        var err: NSDictionary?
        NSAppleScript(source: "do shell script \"\(escaped)\" with administrator privileges")?.executeAndReturnError(&err)
        refresh()
        if let message = err?[NSAppleScript.errorMessage] as? String {
            error = message
        }
    }

    private func run(_ path: String, _ args: [String]) throws -> Data {
        let process = Process()
        let pipe = Pipe()
        process.executableURL = URL(fileURLWithPath: path)
        process.arguments = args
        process.standardOutput = pipe
        try process.run()
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return data
    }

    private func shellQuote(_ s: String) -> String {
        "'" + s.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    /// Bundled next to the app binary first, then the usual install locations.
    private static func findCLI() -> String? {
        let candidates: [String?] = [Bundle.main.path(forResource: "hushup", ofType: nil),
                          "/usr/local/bin/hushup", "/opt/homebrew/bin/hushup"]
        return candidates.compactMap { $0 }.first { FileManager.default.isExecutableFile(atPath: $0) }
    }
}
