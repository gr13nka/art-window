// Port of `WallpaperRenderer` in android/app/src/main/java/dev/artwindow/Wallpaper.kt,
// plus ImageColors.kt (edge and common colours) and PixelBlur.kt (replaced here by
// CIGaussianBlur, which iOS has and Android's older API levels did not).

import Foundation
import ImageIO
import CoreGraphics
import CoreImage
import CoreImage.CIFilterBuiltins

/// Draws a painting for the wallpaper canvas. Owns placement the way `desktop::pin`
/// does: the bitmap comes out at exactly the screen's size, so callers never crop, scale
/// or position anything themselves.
///
/// A full painting is never decoded at full size: ImageIO is asked for a downsample at
/// the size the drawing needs, which is what keeps a 6000 px canvas inside the memory an
/// intent or a widget process is allowed.
public enum Renderer {
    private struct RenderError: LocalizedError {
        let errorDescription: String?
    }

    /// The radius of the blur at `blurStrength` 100, in pixels of the 256 px blur canvas.
    private static let maxBlurRadius = 24.0
    private static let blurShortEdge = 256
    private static let colourSampleSize = 160

    public static func render(imageAt url: URL, style: RenderStyle, screen: Screen) throws -> CGImage {
        guard FileManager.default.fileExists(atPath: url.path) else {
            throw RenderError(errorDescription: "\(url.lastPathComponent) is no longer available")
        }
        guard let size = pixelSize(of: url) else {
            throw RenderError(errorDescription: "\(url.lastPathComponent) could not be read as an image")
        }
        let tooSmall = RenderError(errorDescription:
            "\(url.lastPathComponent) (\(size.width)x\(size.height)) is too small for \(screen.width)x\(screen.height)")
        let canvas = try Canvas(width: screen.width, height: screen.height)
        let full = CGRect(x: 0, y: 0, width: screen.width, height: screen.height)

        switch style.mode {
        case .zoom:
            guard screen.cover(width: size.width, height: size.height) != nil else { throw tooSmall }
            let rect = aspectFill(size, into: full)
            canvas.draw(try downsample(url, maxPixel: Int(max(rect.width, rect.height).rounded(.up))), in: rect)

        case .stretch:
            guard screen.canStretch(width: size.width, height: size.height) else { throw tooSmall }
            canvas.draw(try downsample(url, maxPixel: max(screen.width, screen.height)), in: full)

        case .blur:
            // The blur is made on a small canvas and scaled up: a blur hides detail, so
            // there is nothing to gain from blurring 2500 px rows.
            let smallWidth = min(screen.width, blurShortEdge)
            let smallHeight = max(1, Int((Double(smallWidth) * Double(screen.height) / Double(screen.width)).rounded()))
            let small = CGRect(x: 0, y: 0, width: smallWidth, height: smallHeight)
            let smallCanvas = try Canvas(width: smallWidth, height: smallHeight)
            let coverRect = aspectFill(size, into: small)
            smallCanvas.draw(try downsample(url, maxPixel: Int(max(coverRect.width, coverRect.height).rounded(.up))), in: coverRect)
            let radius = (maxBlurRadius * Double(min(max(style.blurStrength, 0), 100)) / 100).rounded()
            canvas.draw(try blurred(smallCanvas.image(), radius: radius), in: full)

            if style.blurWholeFill {
                guard screen.cover(width: size.width, height: size.height) != nil else { throw tooSmall }
            } else {
                guard screen.fit(width: size.width, height: size.height) != nil else { throw tooSmall }
                let rect = aspectFit(size, into: full)
                canvas.draw(try downsample(url, maxPixel: Int(max(rect.width, rect.height).rounded(.up))), in: rect)
            }

        case .borders:
            guard screen.fit(width: size.width, height: size.height) != nil else { throw tooSmall }
            switch style.border {
            case .black: break
            case .custom(let red, let green, let blue): canvas.fill(red: red, green: green, blue: blue)
            case .edge:
                let sample = try pixels(of: try downsample(url, maxPixel: colourSampleSize))
                let (r, g, b) = channels(edgeAverageColour(sample.pixels, width: sample.width, height: sample.height))
                canvas.fill(red: Double(r) / 255, green: Double(g) / 255, blue: Double(b) / 255)
            }
            let rect = aspectFit(size, into: full)
            canvas.draw(try downsample(url, maxPixel: Int(max(rect.width, rect.height).rounded(.up))), in: rect)
        }
        return try canvas.image()
    }

    /// A small decoded copy, for a widget or a grid, without ever decoding the full picture.
    public static func thumbnail(imageAt url: URL, maxPixel: Int) -> CGImage? {
        try? downsample(url, maxPixel: maxPixel)
    }

    /// Up to five visibly distinct, most-common colours, for the border colour picker.
    public static func edgeColours(imageAt url: URL) -> [RenderStyle.Border] {
        guard let image = try? downsample(url, maxPixel: colourSampleSize),
              let sample = try? pixels(of: image) else { return [] }
        return commonImageColours(sample.pixels, limit: 5).map { colour in
            let (r, g, b) = channels(colour)
            return .custom(red: Double(r) / 255, green: Double(g) / 255, blue: Double(b) / 255)
        }
    }

    // MARK: Geometry

    /// Centred aspect-fill rectangle in top-left coordinates, unbounded — the enlargement
    /// limit is `Screen`'s to judge, not the drawing's.
    private static func aspectFill(_ size: (width: Int, height: Int), into box: CGRect) -> CGRect {
        let scale = max(box.width / CGFloat(size.width), box.height / CGFloat(size.height))
        return centred(CGFloat(size.width) * scale, CGFloat(size.height) * scale, in: box)
    }

    private static func aspectFit(_ size: (width: Int, height: Int), into box: CGRect) -> CGRect {
        let scale = min(box.width / CGFloat(size.width), box.height / CGFloat(size.height))
        return centred((CGFloat(size.width) * scale).rounded(), (CGFloat(size.height) * scale).rounded(), in: box)
    }

    private static func centred(_ width: CGFloat, _ height: CGFloat, in box: CGRect) -> CGRect {
        CGRect(x: box.minX + (box.width - width) / 2, y: box.minY + (box.height - height) / 2, width: width, height: height)
    }

    // MARK: ImageIO

    /// Pixel size from the file header alone, with EXIF orientation applied.
    static func pixelSize(of url: URL) -> (width: Int, height: Int)? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? Int,
              let height = properties[kCGImagePropertyPixelHeight] as? Int,
              width > 0, height > 0
        else { return nil }
        let orientation = properties[kCGImagePropertyOrientation] as? Int ?? 1
        return (5...8).contains(orientation) ? (height, width) : (width, height)
    }

    private static func downsample(_ url: URL, maxPixel: Int) throws -> CGImage {
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: max(1, maxPixel),
        ]
        guard let source = CGImageSourceCreateWithURL(url as CFURL, [kCGImageSourceShouldCache: false] as CFDictionary),
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
        else { throw RenderError(errorDescription: "\(url.lastPathComponent) could not be decoded") }
        return image
    }

    // MARK: Drawing

    /// An opaque sRGB bitmap that is drawn into in top-left coordinates, so the arithmetic
    /// above reads the way `Screen` does rather than upside down.
    private struct Canvas {
        let context: CGContext
        let height: Int

        init(width: Int, height: Int) throws {
            guard let context = CGContext(
                data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                space: CGColorSpace(name: CGColorSpace.sRGB)!,
                bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue
            ) else { throw RenderError(errorDescription: "Could not allocate a \(width)x\(height) canvas") }
            context.interpolationQuality = .high
            context.setFillColor(red: 0, green: 0, blue: 0, alpha: 1)
            context.fill(CGRect(x: 0, y: 0, width: width, height: height))
            self.context = context
            self.height = height
        }

        func fill(red: Double, green: Double, blue: Double) {
            context.setFillColor(red: red, green: green, blue: blue, alpha: 1)
            context.fill(CGRect(x: 0, y: 0, width: context.width, height: context.height))
        }

        func draw(_ image: CGImage, in topLeftRect: CGRect) {
            context.draw(image, in: CGRect(
                x: topLeftRect.minX, y: CGFloat(height) - topLeftRect.maxY,
                width: topLeftRect.width, height: topLeftRect.height
            ))
        }

        func image() throws -> CGImage {
            guard let image = context.makeImage() else { throw RenderError(errorDescription: "Could not finish the canvas") }
            return image
        }
    }

    /// Gaussian blur with edges clamped rather than faded to transparent, as the box blur
    /// on Android clamps its samples to the border.
    private static func blurred(_ image: CGImage, radius: Double) throws -> CGImage {
        guard radius > 0 else { return image }
        let input = CIImage(cgImage: image)
        let filter = CIFilter.gaussianBlur()
        filter.inputImage = input.clampedToExtent()
        filter.radius = Float(radius)
        guard let output = filter.outputImage?.cropped(to: input.extent),
              let result = CIContext().createCGImage(output, from: input.extent)
        else { throw RenderError(errorDescription: "Could not blur the painting") }
        return result
    }

    /// Packed 0xFFRRGGBB pixels, the form ImageColors.kt works in.
    private static func pixels(of image: CGImage) throws -> (pixels: [UInt32], width: Int, height: Int) {
        let width = image.width, height = image.height
        var data = [UInt32](repeating: 0, count: width * height)
        let drawn = data.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(
                data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                space: CGColorSpace(name: CGColorSpace.sRGB)!,
                bitmapInfo: CGImageAlphaInfo.noneSkipFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue
            ) else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drawn else { throw RenderError(errorDescription: "Could not sample the painting's colours") }
        return (data.map { $0 | 0xFF00_0000 }, width, height)
    }
}

// MARK: - ImageColors.kt

private func channels(_ colour: UInt32) -> (UInt32, UInt32, UInt32) {
    ((colour >> 16) & 0xff, (colour >> 8) & 0xff, colour & 0xff)
}

private func opaque(_ red: UInt32, _ green: UInt32, _ blue: UInt32) -> UInt32 {
    0xFF00_0000 | (red << 16) | (green << 8) | blue
}

/// A uniform colour matched to the outside five percent of an image.
func edgeAverageColour(_ pixels: [UInt32], width: Int, height: Int) -> UInt32 {
    guard width > 0, height > 0, pixels.count >= width * height else { return 0xFF00_0000 }
    let band = max(Int((Double(min(width, height)) * 0.05).rounded(.up)), 1)
    var red: UInt64 = 0, green: UInt64 = 0, blue: UInt64 = 0, count: UInt64 = 0
    for y in 0..<height {
        for x in 0..<width where x < band || x >= width - band || y < band || y >= height - band {
            let (r, g, b) = channels(pixels[y * width + x])
            red += UInt64(r); green += UInt64(g); blue += UInt64(b)
            count += 1
        }
    }
    return opaque(UInt32(red / count), UInt32(green / count), UInt32(blue / count))
}

/// The most frequent, visibly distinct colours in a small sampled bitmap. Colours are
/// bucketed to four bits a channel; ties go to the lower bucket so the answer is stable.
func commonImageColours(_ pixels: [UInt32], limit: Int = 5) -> [UInt32] {
    struct Bucket { var count = 0, red: UInt64 = 0, green: UInt64 = 0, blue: UInt64 = 0 }
    var buckets: [UInt32: Bucket] = [:]
    for pixel in pixels {
        let (r, g, b) = channels(pixel)
        let key = ((r >> 4) << 8) | ((g >> 4) << 4) | (b >> 4)
        buckets[key, default: Bucket()].count += 1
        buckets[key]!.red += UInt64(r)
        buckets[key]!.green += UInt64(g)
        buckets[key]!.blue += UInt64(b)
    }

    let minDistanceSquared = 42 * 42
    var result: [UInt32] = []
    let ordered = buckets.sorted { $0.value.count != $1.value.count ? $0.value.count > $1.value.count : $0.key < $1.key }
    for (_, bucket) in ordered {
        let n = UInt64(bucket.count)
        let candidate = opaque(UInt32(bucket.red / n), UInt32(bucket.green / n), UInt32(bucket.blue / n))
        if result.contains(where: { distanceSquared($0, candidate) < minDistanceSquared }) { continue }
        result.append(candidate)
        if result.count == limit { break }
    }
    return result
}

private func distanceSquared(_ a: UInt32, _ b: UInt32) -> Int {
    let (ar, ag, ab) = channels(a), (br, bg, bb) = channels(b)
    let dr = Int(ar) - Int(br), dg = Int(ag) - Int(bg), db = Int(ab) - Int(bb)
    return dr * dr + dg * dg + db * db
}
