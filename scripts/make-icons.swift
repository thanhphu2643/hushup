// Draws the HushUp icons from code so there are no binary sources to keep in sync.
// Usage: swift scripts/make-icons.swift app/Resources
// Output: AppIcon.icns, menubar-on(.png|@2x.png), menubar-off(.png|@2x.png)
import AppKit

let out = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? "app/Resources")

/// Bell glyph in a unit box (AppKit coordinates, y up), unioned into one shape
/// so overlapping parts don't punch holes under the nonzero fill rule.
func bellPath(in r: CGRect) -> CGPath {
    func p(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: r.minX + x * r.width, y: r.minY + y * r.height) }
    func box(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat) -> CGRect {
        CGRect(origin: p(x, y), size: CGSize(width: r.width * w, height: r.height * h))
    }
    // Dome with a slight flare toward the rim.
    let dome = CGMutablePath()
    dome.move(to: p(0.18, 0.30))
    dome.addCurve(to: p(0.28, 0.56), control1: p(0.25, 0.35), control2: p(0.27, 0.45))
    dome.addCurve(to: p(0.50, 0.81), control1: p(0.29, 0.72), control2: p(0.38, 0.81))
    dome.addCurve(to: p(0.72, 0.56), control1: p(0.62, 0.81), control2: p(0.71, 0.72))
    dome.addCurve(to: p(0.82, 0.30), control1: p(0.73, 0.45), control2: p(0.75, 0.35))
    dome.closeSubpath()
    let rim = CGPath(roundedRect: box(0.13, 0.25, 0.74, 0.08), cornerWidth: r.width * 0.04, cornerHeight: r.height * 0.04, transform: nil)
    let knob = CGPath(ellipseIn: box(0.455, 0.78, 0.09, 0.09), transform: nil)
    let clapper = CGPath(ellipseIn: box(0.42, 0.13, 0.16, 0.16), transform: nil)
    return dome.union(rim).union(knob).union(clapper)
}

/// Bell, optionally crossed out with a slash that cuts a gap through it.
func drawGlyph(_ ctx: CGContext, in r: CGRect, slashed: Bool, fill: (CGContext, CGPath) -> Void) {
    let from = CGPoint(x: r.minX + r.width * 0.17, y: r.minY + r.height * 0.85)
    let to = CGPoint(x: r.minX + r.width * 0.83, y: r.minY + r.height * 0.15)
    let w = r.width * 0.075
    ctx.beginTransparencyLayer(auxiliaryInfo: nil)
    fill(ctx, bellPath(in: r))
    if slashed {
        ctx.setLineCap(.round)
        ctx.setBlendMode(.clear)
        ctx.setLineWidth(w * 2)
        ctx.strokeLineSegments(between: [from, to])
        ctx.setBlendMode(.normal)
        let slash = CGMutablePath()
        slash.addLines(between: [from, to])
        fill(ctx, slash.copy(strokingWithWidth: w, lineCap: .round, lineJoin: .round, miterLimit: 1))
    }
    ctx.endTransparencyLayer()
}

func render(px: Int, _ draw: (CGContext, CGFloat) -> Void) -> Data {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8,
                               samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    draw(NSGraphicsContext.current!.cgContext, CGFloat(px))
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

func color(_ hex: UInt32, _ a: CGFloat = 1) -> CGColor {
    CGColor(srgbRed: CGFloat(hex >> 16 & 0xFF) / 255, green: CGFloat(hex >> 8 & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255, alpha: a)
}

func linear(_ ctx: CGContext, _ path: CGPath, _ top: CGColor, _ bottom: CGColor, _ box: CGRect) {
    let g = CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB), colors: [top, bottom] as CFArray, locations: [0, 1])!
    ctx.saveGState()
    ctx.addPath(path)
    ctx.clip()
    ctx.drawLinearGradient(g, start: CGPoint(x: box.midX, y: box.maxY), end: CGPoint(x: box.midX, y: box.minY), options: [])
    ctx.restoreGState()
}

/// macOS Big Sur+ grid: 824pt squircle centred on a 1024 canvas, soft drop shadow.
func appIcon(_ ctx: CGContext, _ size: CGFloat) {
    let s = size / 1024
    let tile = CGRect(x: 100 * s, y: 100 * s, width: 824 * s, height: 824 * s)
    let squircle = CGPath(roundedRect: tile, cornerWidth: 185 * s, cornerHeight: 185 * s, transform: nil)

    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -12 * s), blur: 28 * s, color: color(0x000000, 0.35))
    ctx.addPath(squircle)
    ctx.setFillColor(color(0x312E81))
    ctx.fillPath()
    ctx.restoreGState()

    linear(ctx, squircle, color(0x6D6AF6), color(0x221D5C), tile)
    let glyph = CGRect(x: 232 * s, y: 222 * s, width: 560 * s, height: 560 * s)
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -8 * s), blur: 24 * s, color: color(0x0F0A3C, 0.45))
    drawGlyph(ctx, in: glyph, slashed: true) { ctx, path in
        linear(ctx, path, color(0xFFFFFF), color(0xC7D2FE), glyph)
    }
    ctx.restoreGState()
}

func menuBarIcon(slashed: Bool) -> (CGContext, CGFloat) -> Void {
    { ctx, size in
        let inset = size * 0.04
        drawGlyph(ctx, in: CGRect(x: inset, y: inset, width: size - 2 * inset, height: size - 2 * inset), slashed: slashed) { ctx, path in
            ctx.addPath(path)
            ctx.setFillColor(.black) // template image: macOS tints it
            ctx.fillPath()
        }
    }
}

try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)

let iconset = FileManager.default.temporaryDirectory.appendingPathComponent("AppIcon.iconset")
try? FileManager.default.removeItem(at: iconset)
try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
for pt in [16, 32, 128, 256, 512] {
    try render(px: pt, appIcon).write(to: iconset.appendingPathComponent("icon_\(pt)x\(pt).png"))
    try render(px: pt * 2, appIcon).write(to: iconset.appendingPathComponent("icon_\(pt)x\(pt)@2x.png"))
}
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", iconset.path, "-o", out.appendingPathComponent("AppIcon.icns").path]
try iconutil.run()
iconutil.waitUntilExit()

for (name, slashed) in [("menubar-on", true), ("menubar-off", false)] {
    try render(px: 18, menuBarIcon(slashed: slashed)).write(to: out.appendingPathComponent("\(name).png"))
    try render(px: 36, menuBarIcon(slashed: slashed)).write(to: out.appendingPathComponent("\(name)@2x.png"))
}
try render(px: 1024, appIcon).write(to: out.appendingPathComponent("../../docs/icon.png"))
print("icons written to \(out.path)")
