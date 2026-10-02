// Port of android/app/src/main/java/dev/artwindow/Screen.kt (and `Context.screen()` from
// Wallpaper.kt) — the rules must stay in step.

import Foundation
#if canImport(UIKit)
import UIKit
#endif

/// An axis-aligned box in scaled-image pixels.
public struct Box: Equatable, Sendable {
    public let left: Int, top: Int, right: Int, bottom: Int
}

/// Painting dimensions after aspect-fill scaling, plus the centred screen-sized `crop`.
public struct Placement: Equatable, Sendable {
    public let scaledWidth: Int, scaledHeight: Int
    public let crop: Box
}

public struct FitPlacement: Equatable, Sendable {
    public let scaledWidth: Int, scaledHeight: Int
    public let destination: Box
}

/// Screen-relative placement and the default screen-shaped artwork tolerance.
///
/// Keeps the orientation it is given, and every method compares ratios symmetrically,
/// so none of them needs to know which way round it is. On an iPad the wallpaper canvas
/// is a square (see `DeviceScreen`), for which "shaped like this screen" honestly means
/// "roughly square".
public struct Screen: Equatable, Sendable {
    /// Neither axis of a placed image may lose more than this fraction of itself to the crop.
    public static let maxTrim = 0.15
    /// A photograph smaller than the screen by more than this is refused rather than upscaled soft.
    public static let maxEnlargement = 1.25

    public let width: Int
    public let height: Int

    public init(width: Int, height: Int) {
        self.width = width
        self.height = height
    }

    public var aspectRatio: Double { Double(width) / Double(height) }
    public var isLandscape: Bool { width > height }

    /// Whether a picture of these proportions fills the screen within `maxTrim`. Proportions
    /// only, so a web-sized copy can answer for the original before it is downloaded.
    public func holds(_ aspect: Double) -> Bool {
        let a = aspect, b = aspectRatio
        return 1 - min(a, b) / max(a, b) <= Screen.maxTrim
    }

    /// Centred aspect-fill placement, or nil if the picture would be enlarged past `maxEnlargement`.
    public func cover(width w: Int, height h: Int) -> Placement? {
        guard w > 0, h > 0 else { return nil }
        let scale = max(Double(width) / Double(w), Double(height) / Double(h))
        if scale > Screen.maxEnlargement { return nil }

        let scaledWidth = Int((Double(w) * scale).rounded(.up))
        let scaledHeight = Int((Double(h) * scale).rounded(.up))
        let cropLeft = (scaledWidth - width) / 2
        let cropTop = (scaledHeight - height) / 2
        return Placement(
            scaledWidth: scaledWidth,
            scaledHeight: scaledHeight,
            crop: Box(left: cropLeft, top: cropTop, right: cropLeft + width, bottom: cropTop + height)
        )
    }

    /// Centred aspect-fit destination, leaving the remainder for a backdrop or border.
    public func fit(width w: Int, height h: Int) -> FitPlacement? {
        guard w > 0, h > 0 else { return nil }
        let scale = min(Double(width) / Double(w), Double(height) / Double(h))
        if scale > Screen.maxEnlargement { return nil }
        let scaledWidth = min(Int((Double(w) * scale).rounded(.up)), width)
        let scaledHeight = min(Int((Double(h) * scale).rounded(.up)), height)
        let left = (width - scaledWidth) / 2
        let top = (height - scaledHeight) / 2
        return FitPlacement(
            scaledWidth: scaledWidth,
            scaledHeight: scaledHeight,
            destination: Box(left: left, top: top, right: left + scaledWidth, bottom: top + scaledHeight)
        )
    }

    public func canStretch(width w: Int, height h: Int) -> Bool {
        w > 0 && h > 0 && max(Double(width) / Double(w), Double(height) / Double(h)) <= Screen.maxEnlargement
    }
}

/// Where a painting's rectangle sits on the screen once the user has framed it. The one
/// geometry: `Renderer` asks it at the screen's size and the Settings preview at its own,
/// so they cannot disagree. Rectangles are in top-left coordinates and may extend past
/// the screen; the screen crops them.
public enum Framing {
    /// The size the painting has at zoom 1: filling the screen, or fitted inside it.
    public enum Base: Sendable { case cover, fit }

    public static let maxZoom = 3.0

    /// `painting` is the size as hung. `zoom` (1...`maxZoom`) multiplies the base scale; on
    /// an axis where the result is larger than the screen, `pan` (0...1) places the
    /// screen's window: 0 puts the painting's left (or top) edge at the screen's edge, 1 its
    /// right (or bottom), 0.5 centres. On an axis that does not overflow the painting is
    /// centred. Zoom 1 centred is exactly `Screen.cover` / `Screen.fit`.
    public static func rect(
        painting: CGSize, screen: CGSize, base: Base, zoom: Double = 1, panX: Double = 0.5, panY: Double = 0.5
    ) -> CGRect {
        guard painting.width > 0, painting.height > 0 else { return CGRect(origin: .zero, size: screen) }
        let sx = screen.width / painting.width, sy = screen.height / painting.height
        let scale = (base == .cover ? max(sx, sy) : min(sx, sy)) * CGFloat(min(max(zoom, 1), maxZoom))
        let (w, h) = (painting.width * scale, painting.height * scale)
        return CGRect(x: offset(w, in: screen.width, pan: panX), y: offset(h, in: screen.height, pan: panY), width: w, height: h)
    }

    private static func offset(_ extent: CGFloat, in screen: CGFloat, pan: Double) -> CGFloat {
        let pan = CGFloat(min(max(pan.isFinite ? pan : 0.5, 0), 1))
        return extent > screen + 0.001 ? -(extent - screen) * pan : (screen - extent) / 2
    }
}

/// The wallpaper canvas of this device, in pixels.
///
/// An iPhone's wallpaper is its native portrait pixels. An iPad has one wallpaper for
/// both orientations, so the canvas is a square of the long side: whichever way the
/// iPad is held, the picture is cropped or letterboxed by the system to a window onto
/// the same square. The value is remembered in the shared defaults because a Shortcuts
/// intent runs off the main thread with no `UIScreen` to ask.
public enum DeviceScreen {
    private static let widthKey = "deviceScreenWidth"
    private static let heightKey = "deviceScreenHeight"
    private static let isPadKey = "deviceIsPad"

    @MainActor public static func refresh() {
        #if canImport(UIKit)
        let native = UIScreen.main.nativeBounds
        let short = Int(min(native.width, native.height))
        let long = Int(max(native.width, native.height))
        let pad = UIDevice.current.userInterfaceIdiom == .pad
        let defaults = SharedContainer.defaults
        defaults.set(pad ? long : short, forKey: widthKey)
        defaults.set(long, forKey: heightKey)
        defaults.set(pad, forKey: isPadKey)
        #endif
    }

    public static var current: Screen {
        let defaults = SharedContainer.defaults
        let width = defaults.integer(forKey: widthKey)
        let height = defaults.integer(forKey: heightKey)
        if width > 0, height > 0 { return Screen(width: width, height: height) }
        // Never refreshed: a plausible modern iPhone beats an empty catalogue.
        return Screen(width: 1179, height: 2556)
    }

    public static var isPad: Bool { SharedContainer.defaults.bool(forKey: isPadKey) }
}
