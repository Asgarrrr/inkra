// Probes the running editor's macOS accessibility tree for the attributes a
// screen reader needs to read a document. See docs/gpui-accessibility.md.
//
//   cargo run -p gpui-test-app &
//   swiftc -O scripts/ax-text-probe.swift -o /tmp/ax-text-probe
//   /tmp/ax-text-probe $(pgrep -n gpui-test-app)
//
// Requires Accessibility permission for the terminal in
// System Settings > Privacy & Security > Accessibility.

import Foundation
import ApplicationServices

func attr(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
    var value: CFTypeRef?
    return AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success ? value : nil
}

func role(_ element: AXUIElement) -> String {
    attr(element, kAXRoleAttribute as String) as? String ?? "?"
}

/// Depth-first search for the first node whose role is text-bearing.
func findTextNode(_ element: AXUIElement) -> AXUIElement? {
    let r = role(element)
    if r == "AXTextArea" || r == "AXTextField" { return element }
    if let children = attr(element, kAXChildrenAttribute as String) as? [AXUIElement] {
        for child in children {
            if let hit = findTextNode(child) { return hit }
        }
    }
    return nil
}

guard AXIsProcessTrusted(), CommandLine.arguments.count > 1,
      let pid = Int32(CommandLine.arguments[1]) else {
    print("need Accessibility permission and a pid")
    exit(2)
}

let app = AXUIElementCreateApplication(pid)

// The tree is built asynchronously; poll rather than guess a delay.
var textNode: AXUIElement?
for attempt in 1...20 {
    if let hit = findTextNode(app) {
        print("found text node after \(attempt) poll(s) (~\(attempt) s)")
        textNode = hit
        break
    }
    Thread.sleep(forTimeInterval: 1.0)
}

guard let node = textNode else {
    print("NO AXTextArea/AXTextField ever appeared in the tree")
    exit(1)
}

print("role: \(role(node))")
print("title: \(attr(node, kAXTitleAttribute as String) as? String ?? "nil")")
print("")
print("--- what a screen reader would read ---")

for name in ["AXValue", "AXNumberOfCharacters", "AXSelectedText", "AXSelectedTextRange",
             "AXVisibleCharacterRange", "AXInsertionPointLineNumber"] {
    guard let raw = attr(node, name) else {
        print("\(name): ABSENT")
        continue
    }
    if let text = raw as? String {
        print("\(name): \"\(text)\"")
    } else if let number = raw as? Int {
        print("\(name): \(number)")
    } else if CFGetTypeID(raw) == AXValueGetTypeID() {
        var range = CFRange()
        if AXValueGetValue(raw as! AXValue, .cfRange, &range) {
            print("\(name): location=\(range.location) length=\(range.length)")
        } else {
            print("\(name): (opaque AXValue)")
        }
    } else {
        print("\(name): \(raw)")
    }
}

print("")
let focused = attr(app, kAXFocusedUIElementAttribute as String)
print("AXFocusedUIElement role: \(focused.map { role($0 as! AXUIElement) } ?? "none")")
