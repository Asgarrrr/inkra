// The floor for Inkra's launch: a bare AppKit window that clears one Metal
// frame, with nothing else. See docs/startup-floor.md.
//
//   swiftc -O scripts/appkit-floor.swift -o /tmp/appkit-floor
//   swiftc -O scripts/window-onscreen.swift -o /tmp/window-onscreen
//   /tmp/window-onscreen 8 /tmp/appkit-floor
//
// Each step prints `probe: <step> <epoch µs>` on stderr.

import AppKit
import Metal
import QuartzCore

func now() -> Double { Date().timeIntervalSince1970 * 1e6 }

final class Delegate: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    var layer: CAMetalLayer!
    var device: MTLDevice!
    var queue: MTLCommandQueue!
    func applicationDidFinishLaunching(_ n: Notification) {
        FileHandle.standardError.write("probe: didFinishLaunching \(now())\n".data(using: .utf8)!)
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1000, height: 640),
                          styleMask: [.titled, .closable, .resizable, .miniaturizable],
                          backing: .buffered, defer: false)
        window.center()
        FileHandle.standardError.write("probe: windowCreated \(now())\n".data(using: .utf8)!)
        device = MTLCreateSystemDefaultDevice()
        queue = device.makeCommandQueue()
        let view = NSView(frame: window.contentView!.bounds)
        view.wantsLayer = true
        layer = CAMetalLayer()
        layer.device = device
        layer.pixelFormat = .bgra8Unorm
        layer.drawableSize = CGSize(width: 2000, height: 1280)
        view.layer = layer
        window.contentView = view
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        FileHandle.standardError.write("probe: ordered \(now())\n".data(using: .utf8)!)
        DispatchQueue.main.async { self.draw() }
    }
    func draw() {
        guard let drawable = layer.nextDrawable() else { return }
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = drawable.texture
        pass.colorAttachments[0].loadAction = .clear
        pass.colorAttachments[0].clearColor = MTLClearColor(red: 1, green: 1, blue: 1, alpha: 1)
        pass.colorAttachments[0].storeAction = .store
        let cb = queue.makeCommandBuffer()!
        cb.makeRenderCommandEncoder(descriptor: pass)!.endEncoding()
        drawable.addPresentedHandler { d in
            FileHandle.standardError.write("probe: presented \(now())\n".data(using: .utf8)!)
        }
        cb.present(drawable)
        cb.commit()
        FileHandle.standardError.write("probe: committed \(now())\n".data(using: .utf8)!)
    }
}

FileHandle.standardError.write("probe: main \(now())\n".data(using: .utf8)!)
let app = NSApplication.shared
FileHandle.standardError.write("probe: sharedApp \(now())\n".data(using: .utf8)!)
app.setActivationPolicy(.regular)
let d = Delegate()
app.delegate = d
app.run()
