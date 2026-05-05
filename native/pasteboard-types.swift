import AppKit

let pb = NSPasteboard.general
let types = pb.types?.map { $0.rawValue } ?? []
let json = "{\"types\":[" + types.map { "\"\($0)\"" }.joined(separator: ",") + "]}"
print(json)
