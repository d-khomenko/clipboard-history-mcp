import AppKit
import Foundation

let pb = NSPasteboard.general
let types = pb.types?.map { $0.rawValue } ?? []
let data = try! JSONSerialization.data(withJSONObject: ["types": types])
print(String(data: data, encoding: .utf8)!)
