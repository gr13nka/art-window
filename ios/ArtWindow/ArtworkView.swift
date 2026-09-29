import ArtWindowKit
import SwiftUI

/// The painting, hung alone. Everything else on the screen is about getting the next one.
struct ArtworkView: View {
    let model: AppModel
    let showFavourites: () -> Void

    private var status: RotationStatus { .shared }

    var body: some View {
        VStack(spacing: 16) {
            if let shown = model.shown {
                LoadedImage(url: shown.fileURL, maxPixel: 2400)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .accessibilityLabel(shown.title)
                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(shown.title).font(.headline).lineLimit(2)
                        Text(shown.caption).font(.subheadline).foregroundStyle(.secondary).lineLimit(2)
                    }
                    Spacer(minLength: 12)
                    Button(action: model.toggleFavourite) {
                        Image(systemName: model.isFavourite ? "heart.fill" : "heart").font(.title2)
                    }
                    .accessibilityLabel(model.isFavourite ? "Remove from favourites" : "Add to favourites")
                }
            } else {
                Spacer()
                Text("No painting yet").foregroundStyle(.secondary)
                Spacer()
            }

            VStack(spacing: 10) {
                Button(action: model.next) {
                    Text(status.isFetching ? "Fetching…" : "Next picture").frame(width: 220)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .disabled(status.isFetching)

                progress.frame(width: 220)
                Text(status.statusLine)
                    .font(.footnote).foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)

                Button("Favourites", action: showFavourites).font(.footnote)
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 12)
    }

    /// Choosing is a local filter with no length to report, so it only spins; the bar
    /// gains a position once bytes are arriving. It is drawn even when idle so the
    /// button does not jump when a fetch starts.
    @ViewBuilder private var progress: some View {
        switch status.progress {
        case .idle:
            ProgressView(value: 0).opacity(0)
        case .choosing:
            ProgressView()
                .progressViewStyle(.linear)
        case .downloading(let received, let total):
            if let total, total > 0 {
                ProgressView(value: Double(min(received, total)), total: Double(total))
            } else {
                ProgressView().progressViewStyle(.linear)
            }
        }
    }
}
