import ArtWindowKit
import SwiftUI

/// The shelf. Thumbnails are cached under `Favourite.key`, never by position, so a
/// grid rebuilt around a deletion cannot pair a painting with somebody else's picture.
struct FavouritesView: View {
    @Environment(\.dismiss) private var dismiss
    @State private var items = Favourites.shared.all()
    @State private var thumbnails: [String: UIImage] = [:]
    @State private var selected: Favourite?

    private let columns = [GridItem(.adaptive(minimum: 110), spacing: 8)]

    var body: some View {
        NavigationStack {
            ScrollView {
                if items.isEmpty {
                    Text("Tap the heart on a painting to keep it here.")
                        .foregroundStyle(.secondary).padding(.top, 80)
                } else {
                    LazyVGrid(columns: columns, spacing: 8) {
                        ForEach(items) { item in
                            Button { selected = item } label: { cell(item) }.buttonStyle(.plain)
                        }
                    }
                    .padding(8)
                }
            }
            .background(Color.wall)
            .navigationTitle("Favourites")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
            .sheet(item: $selected) { item in
                FavouriteDetail(item: item, onShown: { selected = nil; dismiss() }, onRemoved: { remove(item) })
            }
        }
    }

    private func cell(_ item: Favourite) -> some View {
        ZStack {
            Color.black
            if let image = thumbnails[item.key] {
                Image(uiImage: image).resizable().aspectRatio(contentMode: .fill)
            }
        }
        .aspectRatio(1, contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .accessibilityLabel(item.artwork.title)
        .task(id: item.key) {
            guard thumbnails[item.key] == nil else { return }
            // A cell is the size of one column, at up to 3x.
            let url = item.artwork.fileURL
            let image = await Task.detached(priority: .utility) {
                Renderer.thumbnail(imageAt: url, maxPixel: 360).map { UIImage(cgImage: $0) }
            }.value
            if let image { thumbnails[item.key] = image }
        }
    }

    private func remove(_ item: Favourite) {
        Favourites.shared.forget(item.key)
        // Whatever is on the desktop stays, the same rule the source's sweep follows.
        Favourites.shared.discardAllBut(shown: StateStore.shared.current.shown)
        thumbnails[item.key] = nil
        items = Favourites.shared.all()
        selected = nil
    }
}

private struct FavouriteDetail: View {
    let item: Favourite
    let onShown: () -> Void
    let onRemoved: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            LoadedImage(url: item.artwork.fileURL, maxPixel: 1600)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            VStack(spacing: 2) {
                Text(item.artwork.title).font(.headline).multilineTextAlignment(.center)
                Text(item.artwork.caption).font(.subheadline).foregroundStyle(.secondary)
            }
            HStack(spacing: 12) {
                Button("Show today") {
                    Task {
                        await Rotation.shared.show(item.artwork)
                        onShown()
                    }
                }
                .buttonStyle(.borderedProminent)
                Button("Remove", role: .destructive, action: onRemoved)
                    .buttonStyle(.bordered)
            }
            .controlSize(.large)
        }
        .padding(20)
        .background(Color.wall.ignoresSafeArea())
        .presentationDetents([.large])
    }
}
