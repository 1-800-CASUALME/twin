#!/usr/bin/env bash
# Build Twin.app: release twin CLI + SwiftUI app, assembled into mac/build/Twin.app
set -euo pipefail
cd "$(dirname "$0")"
( cd ../core && cargo build --release -p twin )
swift build -c release
APP=build/Twin.app
rm -rf "$APP"; mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp .build/release/Twin "$APP/Contents/MacOS/Twin"
cp ../core/target/release/twin "$APP/Contents/MacOS/twin-cli"   # not "twin": APFS is case-insensitive and would clobber Twin
cp Info.plist "$APP/Contents/Info.plist"
# icon: render the app symbol into an .icns if the tools are present
if command -v iconutil >/dev/null && command -v swift >/dev/null; then
  ICONSET=build/Twin.iconset; rm -rf "$ICONSET"; mkdir -p "$ICONSET"
  cat > build/icon.swift <<'SW'
import AppKit
let sizes = [16, 32, 64, 128, 256, 512, 1024]
for s in sizes {
  let img = NSImage(size: NSSize(width: s, height: s))
  img.lockFocus()
  let r = NSRect(x: 0, y: 0, width: s, height: s)
  let path = NSBezierPath(roundedRect: r.insetBy(dx: CGFloat(s)*0.04, dy: CGFloat(s)*0.04), xRadius: CGFloat(s)*0.22, yRadius: CGFloat(s)*0.22)
  let g = NSGradient(starting: NSColor(calibratedRed: 0.20, green: 0.50, blue: 1.0, alpha: 1), ending: NSColor(calibratedRed: 0.10, green: 0.25, blue: 0.75, alpha: 1))!
  g.draw(in: path, angle: -90)
  let cfg = NSImage.SymbolConfiguration(pointSize: CGFloat(s)*0.52, weight: .medium)
  if let sym = NSImage(systemSymbolName: "arrow.triangle.2.circlepath", accessibilityDescription: nil)?.withSymbolConfiguration(cfg) {
    let tinted = NSImage(size: sym.size, flipped: false) { rect in
      sym.draw(in: rect); NSColor.white.set(); rect.fill(using: .sourceAtop); return true }
    let sz = tinted.size
    tinted.draw(in: NSRect(x: (CGFloat(s)-sz.width)/2, y: (CGFloat(s)-sz.height)/2, width: sz.width, height: sz.height))
  }
  img.unlockFocus()
  let rep = NSBitmapImageRep(data: img.tiffRepresentation!)!
  let png = rep.representation(using: .png, properties: [:])!
  let name = s == 1024 ? "icon_512x512@2x.png" : (s == 64 ? "icon_32x32@2x.png" : "icon_\(s)x\(s).png")
  try! png.write(to: URL(fileURLWithPath: CommandLine.arguments[1] + "/" + name))
}
SW
  swift build/icon.swift "$ICONSET" 2>/dev/null && iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Twin.icns" || echo "icon skipped"
fi
codesign --force --deep -s - "$APP" 2>/dev/null || true
echo "built $APP"
