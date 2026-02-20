import AppKit

let size = NSSize(width: 1024, height: 1024)
let image = NSImage(size: size)
image.lockFocus()

let rect = NSRect(origin: .zero, size: size)
let bg = NSBezierPath(roundedRect: rect.insetBy(dx: 40, dy: 40), xRadius: 220, yRadius: 220)
let gradient = NSGradient(colors: [
    NSColor(calibratedRed: 0.06, green: 0.10, blue: 0.20, alpha: 1.0),
    NSColor(calibratedRed: 0.16, green: 0.32, blue: 0.58, alpha: 1.0)
])!
gradient.draw(in: bg, angle: 90)

let inner = NSBezierPath(roundedRect: rect.insetBy(dx: 130, dy: 180), xRadius: 80, yRadius: 80)
NSColor(calibratedWhite: 0.08, alpha: 0.9).setFill()
inner.fill()

let prompt = ">_"
let promptAttrs: [NSAttributedString.Key: Any] = [
    .font: NSFont.monospacedSystemFont(ofSize: 170, weight: .bold),
    .foregroundColor: NSColor(calibratedRed: 0.40, green: 0.93, blue: 0.70, alpha: 1.0)
]
let promptSize = prompt.size(withAttributes: promptAttrs)
let promptRect = NSRect(
    x: (size.width - promptSize.width) / 2,
    y: size.height / 2 - 70,
    width: promptSize.width,
    height: promptSize.height
)
prompt.draw(in: promptRect, withAttributes: promptAttrs)

let title = "Orchestra"
let titleAttrs: [NSAttributedString.Key: Any] = [
    .font: NSFont.systemFont(ofSize: 62, weight: .semibold),
    .foregroundColor: NSColor.white
]
let titleSize = title.size(withAttributes: titleAttrs)
let titleRect = NSRect(
    x: (size.width - titleSize.width) / 2,
    y: 220,
    width: titleSize.width,
    height: titleSize.height
)
title.draw(in: titleRect, withAttributes: titleAttrs)

image.unlockFocus()

let tiff = image.tiffRepresentation!
let bitmap = NSBitmapImageRep(data: tiff)!
let data = bitmap.representation(using: .png, properties: [:])!
try data.write(to: URL(fileURLWithPath: "assets/AppIcon.png"))
