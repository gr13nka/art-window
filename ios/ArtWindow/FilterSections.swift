import ArtWindowKit
import SwiftUI

/// Shape, Origins, Subjects and Artists. Sections are ANDed and the options inside one
/// are ORed; an empty selection is that section's **Any**, always the first row.
/// An option the catalogue cannot offer under the other sections is hidden, unless it
/// is already checked, so it can always be unchecked.
struct FilterSections: View {
    @Binding var filters: Filters
    let availability: Availability
    @Binding var foldedArtists: Bool

    @State private var open: Set<String> = ["Shape", "Origins", "Subjects"]

    var body: some View {
        fold("Shape", summary: filters.shape.label(isPad: DeviceScreen.isPad)) {
            ForEach(ArtworkShape.allCases, id: \.self) { shape in
                row(shape.label(isPad: DeviceScreen.isPad), checked: filters.shape == shape) { filters.shape = shape }
            }
        }

        fold("Origins", summary: summary(filters.regions.sorted { $0.rawValue < $1.rawValue }.map(\.label), any: "Any origin", noun: "origins")) {
            row("Any", checked: filters.regions.isEmpty) { filters.regions = [] }
            ForEach(Region.allCases.filter { availability.regions.contains($0) || filters.regions.contains($0) }, id: \.self) { region in
                row(region.label, checked: filters.regions.contains(region)) { toggle(&filters.regions, region) }
            }
        }

        fold("Subjects", summary: summary(filters.subjects.sorted { $0.rawValue < $1.rawValue }.map(\.label), any: "Any subject", noun: "subjects")) {
            row("Any", checked: filters.subjects.isEmpty) { filters.subjects = [] }
            ForEach(ArtworkSubject.allCases.filter { availability.subjects.contains($0) || filters.subjects.contains($0) }, id: \.self) { subject in
                row(subject.label, checked: filters.subjects.contains(subject)) { toggle(&filters.subjects, subject) }
            }
        }

        // Nobody has narrowed Artists on a fresh install, so it starts folded rather than
        // opening a list of hundreds of names with nothing to review in it.
        DisclosureGroup(isExpanded: Binding(get: { !foldedArtists }, set: { foldedArtists = !$0 })) {
            row("Any", checked: filters.artists.isEmpty) { filters.artists = [] }
            ForEach(availability.artists.union(filters.artists).sorted(), id: \.self) { artist in
                row(artist, checked: filters.artists.contains(artist)) { toggle(&filters.artists, artist) }
            }
        } label: {
            header("Artists", summary(filters.artists.sorted(), any: "Any artist", noun: "artists"))
        }
    }

    private func fold(_ title: String, summary: String, @ViewBuilder content: () -> some View) -> some View {
        let content = content()
        return DisclosureGroup(isExpanded: Binding(
            get: { open.contains(title) },
            set: { if $0 { open.insert(title) } else { open.remove(title) } })
        ) {
            content
        } label: {
            header(title, summary)
        }
    }

    private func header(_ title: String, _ summary: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
            Text(summary).font(.footnote).foregroundStyle(.secondary).lineLimit(1)
        }
    }

    private func row(_ title: String, checked: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack {
                Text(title).foregroundStyle(.primary)
                Spacer()
                if checked { Image(systemName: "checkmark").foregroundStyle(Color.accentColor) }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func toggle<T: Hashable>(_ set: inout Set<T>, _ element: T) {
        if !set.insert(element).inserted { set.remove(element) }
    }

    private func summary(_ names: [String], any: String, noun: String) -> String {
        switch names.count {
        case 0: any
        case 1...3: names.joined(separator: ", ")
        default: "\(names.count) \(noun)"
        }
    }
}
