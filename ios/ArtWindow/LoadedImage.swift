import ArtWindowKit
import SwiftUI

/// A painting decoded by ImageIO at no more than `maxPixel`, off the main thread.
/// Callers say how big it will be drawn, so a full-resolution museum JPEG is never
/// held for a view a quarter of an inch wide.
struct LoadedImage: View {
    let url: URL
    let maxPixel: Int
    var contentMode: ContentMode = .fit

    @State private var image: UIImage?

    var body: some View {
        Group {
            if let image {
                Image(uiImage: image).resizable().aspectRatio(contentMode: contentMode)
            } else {
                Color.clear
            }
        }
        .task(id: url) {
            let url = url, maxPixel = maxPixel
            image = await Task.detached(priority: .userInitiated) {
                Renderer.thumbnail(imageAt: url, maxPixel: maxPixel).map { UIImage(cgImage: $0) }
            }.value
        }
    }
}
