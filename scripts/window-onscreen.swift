// Launches a program and reports the ms from spawn until the WindowServer lists
// its first on-screen window: the launch as the user sees it, taken from
// outside the process. See docs/startup-floor.md.
//
//   swiftc -O scripts/window-onscreen.swift -o /tmp/window-onscreen
//   /tmp/window-onscreen 8 "$PWD/target/release/inkra-app" <vault>
//
// Run 0 warms the caches and is dropped.

import Foundation
import CoreGraphics

let args = Array(CommandLine.arguments.dropFirst())
let runs = Int(args[0])!
let cmd = Array(args.dropFirst())
var results: [Double] = []
for r in 0...runs {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: cmd[0])
    p.arguments = Array(cmd.dropFirst())
    p.standardError = FileHandle.nullDevice
    p.standardOutput = FileHandle.nullDevice
    let t0 = Date()
    try! p.run()
    let pid = p.processIdentifier
    var seen: Double? = nil
    while Date().timeIntervalSince(t0) < 4 {
        if let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] {
            if list.contains(where: { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && (($0[kCGWindowLayer as String] as? Int) ?? 1) == 0 }) {
                seen = Date().timeIntervalSince(t0) * 1000; break
            }
        }
        usleep(300)
    }
    usleep(400_000); p.terminate(); p.waitUntilExit(); usleep(800_000)
    if r > 0, let s = seen { results.append(s) }
}
results.sort()
let med = results[results.count / 2]
print(results.map { String(format: "%.0f", $0) }.joined(separator: " "), String(format: "| median %.1f ms", med))
