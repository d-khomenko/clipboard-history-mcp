#!/usr/bin/env swift
// klipta-ocr — VisionKit OCR helper for clipboard-history-mcp.
//
// Usage: klipta-ocr <image-path>
//
// Reads the image at the given path, runs Apple VisionKit text recognition,
// and prints the extracted text to stdout (UTF-8). Exits 0 on success,
// non-zero on failure.
//
// TODO(manual): compile and install via the `install` subcommand:
//   swiftc tools/klipta-ocr.swift -o ~/.local/share/clipboard-history-mcp/bin/klipta-ocr
// Or extend the `install` subcommand in src/cli/ to do this automatically.

import Vision
import Foundation

guard CommandLine.arguments.count >= 2 else {
    fputs("usage: klipta-ocr <image-path>\n", stderr)
    exit(1)
}

let path = CommandLine.arguments[1]
let url = URL(fileURLWithPath: path)

guard let cgImage = { () -> CGImage? in
    guard let src = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
    return CGImageSourceCreateImageAtIndex(src, 0, nil)
}() else {
    fputs("klipta-ocr: could not load image at \(path)\n", stderr)
    exit(2)
}

let request = VNRecognizeTextRequest()
request.recognitionLevel = .accurate
request.usesLanguageCorrection = true

let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
do {
    try handler.perform([request])
} catch {
    fputs("klipta-ocr: vision error: \(error)\n", stderr)
    exit(3)
}

let lines = (request.results ?? []).compactMap { obs -> String? in
    obs.topCandidates(1).first?.string
}
print(lines.joined(separator: "\n"))
