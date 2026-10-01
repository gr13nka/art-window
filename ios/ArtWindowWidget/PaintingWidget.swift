import ArtWindowKit
import SwiftUI
import WidgetKit

/// Widgets never fetch. They draw what the app, the intent or the background refresh
/// left in the shared container, and `Rotation` reloads their timelines when that changes.
struct PaintingEntry: TimelineEntry {
    let date: Date
    let image: UIImage?
    let title: String
    let caption: String

    static let placeholder = PaintingEntry(date: .now, image: nil, title: "", caption: "")

    static func current() -> PaintingEntry {
        let shown = StateStore.shared.current.shown
        return PaintingEntry(
            date: .now,
            image: UIImage(contentsOfFile: SharedContainer.widgetImage.path),
            title: shown?.title ?? "",
            caption: shown?.caption ?? "")
    }
}

struct PaintingProvider: TimelineProvider {
    func placeholder(in context: Context) -> PaintingEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (PaintingEntry) -> Void) {
        completion(context.isPreview ? .placeholder : .current())
    }

    /// One entry that lapses when the next day begins and a new painting may be owed.
    func getTimeline(in context: Context, completion: @escaping (Timeline<PaintingEntry>) -> Void) {
        completion(Timeline(entries: [.current()], policy: .after(Day.nextStart(after: .now))))
    }
}

struct PaintingWidgetView: View {
    let entry: PaintingEntry
    @Environment(\.widgetFamily) private var family

    private var showsCaption: Bool { family == .systemLarge || family == .systemExtraLarge }

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .bottomLeading) {
                Color.black
                if let image = entry.image {
                    Image(uiImage: image)
                        .resizable().aspectRatio(contentMode: .fill)
                        .frame(width: proxy.size.width, height: proxy.size.height)
                        .clipped()
                }
                if showsCaption, !entry.title.isEmpty {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(entry.title).font(.headline).lineLimit(2)
                        Text(entry.caption).font(.caption).lineLimit(1).opacity(0.8)
                    }
                    .foregroundStyle(.white)
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(LinearGradient(colors: [.clear, .black.opacity(0.7)], startPoint: .top, endPoint: .bottom))
                }
            }
        }
        .containerBackground(.black, for: .widget)
    }
}

@main
struct PaintingWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "PaintingWidget", provider: PaintingProvider()) { entry in
            PaintingWidgetView(entry: entry)
        }
        .configurationDisplayName("Today's painting")
        .description("The painting Art Window chose for today.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge, .systemExtraLarge])
        // The painting runs to the edge of the widget.
        .contentMarginsDisabled()
    }
}
