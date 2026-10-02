import ArtWindowKit
import SwiftUI

/// The painters, one picture each, laid out as `FavouritesView` lays out the shelf.
/// Choosing only stages the name in Settings' filters (a binding, not a copy); *Apply
/// changes* there is still what commits it. Mirrors `Pending::artist_cards` on the
/// desktop and the Android copy.
struct ArtistBrowser: View {
    @Environment(\.dismiss) private var dismiss
    @Binding var chosen: Set<String>
    /// Painters the other staged filters still leave twenty paintings of.
    let available: Set<String>
    /// The staged filters, to tell why a painter has nothing left under them.
    let filters: Filters

    struct Card: Identifiable {
        let painter: Painter
        let paintings: Int
        /// Set when Subject or *Hide religious* leaves the painter nothing.
        let emptied: Catalogue.Emptied?
        var id: String { painter.id }
    }

    @State private var cards: [Card] = []
    @State private var thumbnails: [String: UIImage] = [:]
    @State private var selected: Card?

    private let columns = [GridItem(.adaptive(minimum: 110), spacing: 8)]

    var body: some View {
        NavigationStack {
            ScrollView {
                LazyVGrid(columns: columns, spacing: 8) {
                    ForEach(cards) { card in
                        Button { selected = card } label: { cell(card) }.buttonStyle(.plain)
                    }
                }
                .padding(8)
            }
            .background(Color.wall)
            .navigationTitle("Artists")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
            .sheet(item: $selected) { card in
                ArtistDetail(card: card, chosen: $chosen, close: { selected = nil })
            }
            // Counting walks the whole catalogue, so it never runs on the main thread.
            .task {
                let (available, filters) = (available, filters)
                cards = await Task.detached(priority: .userInitiated) {
                    let catalogue = Catalogue.shared
                    return Artists.shared.shelf(of: catalogue.artists()).map { painter in
                        Card(
                            painter: painter, paintings: catalogue.paintings(by: painter.name),
                            emptied: available.contains(painter.name) ? nil
                                : catalogue.whatEmpties(painter.name, filters, screen: DeviceScreen.current))
                    }
                }.value
            }
        }
    }

    private func cell(_ card: Card) -> some View {
        let painter = card.painter
        return ZStack(alignment: .topTrailing) {
            ZStack {
                Color.black
                if let image = thumbnails[painter.id] {
                    Image(uiImage: image).resizable().aspectRatio(contentMode: .fill)
                }
            }
            .aspectRatio(1, contentMode: .fit)
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .opacity(chosen.contains(painter.name) || card.emptied == nil ? 1 : 0.45)
            if chosen.contains(painter.name) {
                Image(systemName: "checkmark.circle.fill")
                    .foregroundStyle(.white, Color.accentColor).padding(6)
            }
        }
        .accessibilityLabel(painter.name)
        .task(id: painter.id) {
            guard thumbnails[painter.id] == nil else { return }
            // A cell is the size of one column, at up to 3x; the 1400 px file is never held.
            let url = painter.imageURL
            let image = await Task.detached(priority: .utility) {
                Renderer.thumbnail(imageAt: url, maxPixel: 360).map { UIImage(cgImage: $0) }
            }.value
            if let image { thumbnails[painter.id] = image }
        }
    }
}

private struct ArtistDetail: View {
    let card: ArtistBrowser.Card
    @Binding var chosen: Set<String>
    let close: () -> Void

    private var isChosen: Bool { chosen.contains(card.painter.name) }

    /// A chosen painter can always be removed. Otherwise only Subject and *Hide religious*
    /// can block one, since a chosen painter wins over Shape and Origins and one painting
    /// is enough.
    private var blocked: String? {
        guard !isChosen, let emptied = card.emptied else { return nil }
        switch emptied {
        case .subject: return "None of \(card.painter.name)'s paintings match the chosen subject."
        case .religious: return "None of \(card.painter.name)'s paintings are left with religious scenes hidden."
        }
    }

    var body: some View {
        VStack(spacing: 16) {
            LoadedImage(url: card.painter.imageURL, maxPixel: 1600)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            VStack(spacing: 2) {
                Text(card.painter.name).font(.headline).multilineTextAlignment(.center)
                Text(card.painter.caption(paintings: card.paintings))
                    .font(.subheadline).foregroundStyle(.secondary).multilineTextAlignment(.center)
                if let blocked {
                    Text(blocked).font(.footnote).foregroundStyle(.orange)
                        .multilineTextAlignment(.center).padding(.top, 4)
                }
            }
            HStack(spacing: 12) {
                if isChosen {
                    Button("Remove", role: .destructive) { chosen.remove(card.painter.name); close() }
                        .buttonStyle(.borderedProminent)
                } else {
                    Button("Choose") { chosen.insert(card.painter.name); close() }
                        .buttonStyle(.borderedProminent)
                        .disabled(blocked != nil)
                }
                Link("Read more", destination: card.painter.about)
                    .buttonStyle(.bordered)
            }
            .controlSize(.large)
        }
        .padding(20)
        .background(Color.wall.ignoresSafeArea())
        .presentationDetents([.large])
    }
}
