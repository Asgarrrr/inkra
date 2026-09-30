// Launches a program and measures its whole process tree from outside, then
// prints one JSON line. Metrics and method: docs/benchmarks.md.
//
//   swiftc -O scripts/bench/measure.swift -o target/bench/measure
//   HOME=/tmp/h target/bench/measure [--cap SECONDS] -- <program> [args...]
//
// The program inherits the environment and stderr; its stdout is discarded.

import CoreGraphics
import Darwin
import Foundation

let sampleEvery = 0.25
let quietBelow = 2.0  // percent of one core, as a mean over `meanOver`
let meanOver = 1.0
let quietFor = 3.0
let idleFor = 15.0

var args = Array(CommandLine.arguments.dropFirst())
var cap = 180.0
if args.first == "--cap", args.count > 1, let value = Double(args[1]) {
    cap = value
    args.removeFirst(2)
}
guard args.first == "--", args.count > 1 else {
    FileHandle.standardError.write("usage: measure [--cap SECONDS] -- <program> [args...]\n".data(using: .utf8)!)
    exit(2)
}
let command = Array(args.dropFirst())

var timebase = mach_timebase_info_data_t()
mach_timebase_info(&timebase)
let start = DispatchTime.now().uptimeNanoseconds
func elapsed() -> Double { Double(DispatchTime.now().uptimeNanoseconds - start) / 1e9 }

func children(of pid: pid_t) -> [pid_t] {
    var buffer = [pid_t](repeating: 0, count: 512)
    let count = proc_listchildpids(pid, &buffer, Int32(buffer.count * MemoryLayout<pid_t>.size))
    return count > 0 ? Array(buffer.prefix(Int(count))) : []
}

// Re-listed on every call: Electron spawns its helpers after the window.
func tree(_ root: pid_t) -> [pid_t] {
    var all = [root]
    var index = 0
    while index < all.count {
        all += children(of: all[index])
        index += 1
    }
    return all
}

func usage(_ pid: pid_t) -> rusage_info_v4? {
    var info = rusage_info_v4()
    let status = withUnsafeMutablePointer(to: &info) {
        $0.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) {
            proc_pid_rusage(pid, RUSAGE_INFO_V4, $0)
        }
    }
    return status == 0 ? info : nil
}

struct Sample {
    var cpuSeconds = 0.0  // since the previous sample
    var footprint: UInt64 = 0
    var resident: UInt64 = 0
    var processes = 0
}

// CPU times are in Mach ticks, not ns, on Apple silicon.
var lastCpu: [pid_t: UInt64] = [:]
func sample(_ root: pid_t) -> Sample {
    var result = Sample()
    var cpu: [pid_t: UInt64] = [:]
    for pid in tree(root) {
        guard let info = usage(pid) else { continue }
        let ticks = info.ri_user_time + info.ri_system_time
        cpu[pid] = ticks
        // A process first seen now has run only since the launch, so all of its time counts.
        let delta = ticks - min(lastCpu[pid] ?? 0, ticks)
        result.cpuSeconds += Double(delta) * Double(timebase.numer) / Double(timebase.denom) / 1e9
        result.footprint += info.ri_phys_footprint
        result.resident += info.ri_resident_size
        result.processes += 1
    }
    lastCpu = cpu
    return result
}

func hasWindow(_ pids: Set<pid_t>) -> Bool {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] else {
        return false
    }
    return list.contains {
        guard let owner = $0[kCGWindowOwnerPID as String] as? Int32 else { return false }
        return pids.contains(owner) && ($0[kCGWindowLayer as String] as? Int) == 0
    }
}

let process = Process()
process.executableURL = URL(fileURLWithPath: command[0])
process.arguments = Array(command.dropFirst())
process.standardOutput = FileHandle.nullDevice
try process.run()
let root = process.processIdentifier

// Every sample since spawn: its time and the tree's CPU-seconds so far.
var history: [(at: Double, cpu: Double)] = [(0, 0)]
var windowAt: Double?
// The start of the first 1 s window of the quiet run.
var quietStart: (at: Double, cpu: Double)?
var settled: Sample?
while elapsed() < cap, process.isRunning {
    if windowAt == nil, hasWindow(Set(tree(root))) {
        windowAt = elapsed()
    }
    let now = elapsed()
    if now - history.last!.at >= sampleEvery {
        let current = sample(root)
        let cpu = history.last!.cpu + current.cpuSeconds
        history.append((now, cpu))
        // Only windows after the window is on screen: a launch stalled before it is not idle.
        if let from = history.last(where: { $0.at <= now - meanOver }), let windowAt, from.at >= windowAt,
           (cpu - from.cpu) / (now - from.at) * 100 < quietBelow {
            quietStart = quietStart ?? from
            if now - quietStart!.at >= meanOver + quietFor {
                settled = current
                break
            }
        } else {
            quietStart = nil
        }
    }
    usleep(windowAt == nil ? 1_000 : 20_000)
}

var idleCpu: Double?
if settled != nil {
    let idleStart = elapsed()
    var busy = 0.0
    while elapsed() - idleStart < idleFor, process.isRunning {
        usleep(UInt32(sampleEvery * 1e6))
        busy += sample(root).cpuSeconds
    }
    idleCpu = busy / (elapsed() - idleStart) * 100
}

let exited = !process.isRunning
let family = tree(root)
func alive(_ pid: pid_t) -> Bool { pid == root ? process.isRunning : kill(pid, 0) == 0 }
func waitGone(_ pids: [pid_t]) -> [pid_t] {
    let deadline = elapsed() + 3
    var left = pids.filter(alive)
    while !left.isEmpty, elapsed() < deadline {
        usleep(50_000)
        left = left.filter(alive)
    }
    return left
}
if process.isRunning { kill(root, SIGTERM) }
var survivors = waitGone(family)
survivors.forEach { kill($0, SIGTERM) }
survivors = waitGone(survivors)
survivors.forEach { kill($0, SIGKILL) }
process.waitUntilExit()

func ms(_ seconds: Double?) -> Any { seconds.map { Int(($0 * 1000).rounded()) } ?? NSNull() }
let result: [String: Any] = [
    "window_ms": ms(windowAt),
    "busy_ms": settled != nil ? ms(quietStart?.at) : (exited ? "exited" : "timeout"),
    "cpu_s": (settled != nil ? quietStart : nil).map { NSDecimalNumber(string: String(format: "%.3f", $0.cpu)) } ?? NSNull(),
    "footprint_bytes": settled.map { $0.footprint } ?? NSNull(),
    "resident_bytes": settled.map { $0.resident } ?? NSNull(),
    "processes": settled.map { $0.processes } ?? NSNull(),
    "idle_cpu_pct": idleCpu.map { NSDecimalNumber(string: String(format: "%.2f", $0)) } ?? NSNull(),
]
let json = try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys])
print(String(decoding: json, as: UTF8.self))
