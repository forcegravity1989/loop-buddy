// 把 video-frames/*.png 编成 H.264 mp4 —— 用 macOS 自带的 AVFoundation。
//
// 为什么不是 ffmpeg:本机没装,而验收工具链的原则是不为一件小事改动用户机器
// 状态(同 gen-flow-report.py 不装 python@3.11 的理由)。AVAssetWriter 是系统
// 自带能力,零安装。
//
//   swift scripts/encode-mp4.swift <frames-dir> <out.mp4> [每帧秒数=2.0]
//
// 每帧只写一次,用 PTS 控制停留时长(而不是复制几十份相同帧占满磁盘)。

import AVFoundation
import AppKit
import Foundation

let args = CommandLine.arguments
guard args.count >= 3 else {
    FileHandle.standardError.write("用法:encode-mp4.swift <frames-dir> <out.mp4> [每帧秒数]\n".data(using: .utf8)!)
    exit(1)
}
let framesDir = URL(fileURLWithPath: args[1])
let outURL = URL(fileURLWithPath: args[2])
let holdSeconds = args.count > 3 ? (Double(args[3]) ?? 2.0) : 2.0

let fm = FileManager.default
guard let names = try? fm.contentsOfDirectory(atPath: framesDir.path) else {
    FileHandle.standardError.write("读不到帧目录:\(framesDir.path)\n".data(using: .utf8)!)
    exit(1)
}
let frames = names.filter { $0.hasSuffix(".png") }.sorted()
guard !frames.isEmpty else {
    FileHandle.standardError.write("帧目录里没有 png\n".data(using: .utf8)!)
    exit(1)
}

// 尺寸取第一帧;H.264 要求偶数边长。
guard let first = NSImage(contentsOf: framesDir.appendingPathComponent(frames[0])),
      let firstCG = first.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
    FileHandle.standardError.write("第一帧解不开\n".data(using: .utf8)!)
    exit(1)
}
let width = firstCG.width - (firstCG.width % 2)
let height = firstCG.height - (firstCG.height % 2)

try? fm.removeItem(at: outURL)
let writer = try AVAssetWriter(outputURL: outURL, fileType: .mp4)
let settings: [String: Any] = [
    AVVideoCodecKey: AVVideoCodecType.h264,
    AVVideoWidthKey: width,
    AVVideoHeightKey: height,
    AVVideoCompressionPropertiesKey: [
        AVVideoAverageBitRateKey: 6_000_000,
        AVVideoMaxKeyFrameIntervalKey: 1,
    ],
]
let input = AVAssetWriterInput(mediaType: .video, outputSettings: settings)
input.expectsMediaDataInRealTime = false
let adaptor = AVAssetWriterInputPixelBufferAdaptor(
    assetWriterInput: input,
    sourcePixelBufferAttributes: [
        kCVPixelBufferPixelFormatTypeKey as String: Int(kCVPixelFormatType_32ARGB),
        kCVPixelBufferWidthKey as String: width,
        kCVPixelBufferHeightKey as String: height,
    ]
)
writer.add(input)
writer.startWriting()
writer.startSession(atSourceTime: .zero)

let timescale: CMTimeScale = 600

func pixelBuffer(from cg: CGImage) -> CVPixelBuffer? {
    var pb: CVPixelBuffer?
    let attrs = [kCVPixelBufferCGImageCompatibilityKey: true,
                 kCVPixelBufferCGBitmapContextCompatibilityKey: true] as CFDictionary
    guard CVPixelBufferCreate(kCFAllocatorDefault, width, height,
                              kCVPixelFormatType_32ARGB, attrs, &pb) == kCVReturnSuccess,
          let buf = pb else { return nil }
    CVPixelBufferLockBaseAddress(buf, [])
    defer { CVPixelBufferUnlockBaseAddress(buf, []) }
    guard let ctx = CGContext(
        data: CVPixelBufferGetBaseAddress(buf),
        width: width, height: height, bitsPerComponent: 8,
        bytesPerRow: CVPixelBufferGetBytesPerRow(buf),
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.noneSkipFirst.rawValue
    ) else { return nil }
    ctx.draw(cg, in: CGRect(x: 0, y: 0, width: width, height: height))
    return buf
}

var index = 0
let done = DispatchSemaphore(value: 0)
let queue = DispatchQueue(label: "bw.encode")

input.requestMediaDataWhenReady(on: queue) {
    while input.isReadyForMoreMediaData {
        if index >= frames.count {
            // 末帧多停一拍,播放器不会把最后一格一闪而过。
            input.markAsFinished()
            done.signal()
            return
        }
        let url = framesDir.appendingPathComponent(frames[index])
        guard let img = NSImage(contentsOf: url),
              let cg = img.cgImage(forProposedRect: nil, context: nil, hints: nil),
              let buf = pixelBuffer(from: cg) else {
            FileHandle.standardError.write("跳过解不开的帧:\(frames[index])\n".data(using: .utf8)!)
            index += 1
            continue
        }
        let pts = CMTime(value: CMTimeValue(Double(index) * holdSeconds * Double(timescale)),
                         timescale: timescale)
        if !adaptor.append(buf, withPresentationTime: pts) {
            FileHandle.standardError.write("append 失败:\(writer.error?.localizedDescription ?? "?")\n".data(using: .utf8)!)
        }
        index += 1
    }
}

done.wait()
let finished = DispatchSemaphore(value: 0)
writer.finishWriting { finished.signal() }
finished.wait()

if writer.status == .completed {
    let bytes = (try? fm.attributesOfItem(atPath: outURL.path)[.size] as? Int) ?? 0
    let secs = Double(frames.count) * holdSeconds
    print("[mp4] \(frames.count) 帧 · \(String(format: "%.0f", secs))s · \(width)x\(height) · \(bytes / 1024)KB → \(outURL.path)")
} else {
    FileHandle.standardError.write("编码失败:\(writer.error?.localizedDescription ?? "未知")\n".data(using: .utf8)!)
    exit(1)
}
