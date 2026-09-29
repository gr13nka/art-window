import ArtWindowKit
import SwiftUI

struct RootView: View {
    private enum Page: String, CaseIterable, Identifiable {
        case artwork = "Artwork", settings = "Settings"
        var id: Self { self }
    }

    @State private var model = AppModel()
    @State private var page = Page.artwork
    @State private var showingGuide = !Preferences.shared.hasSeenAutomationGuide
    @State private var showingFavourites = false
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        ZStack {
            Color.wall.ignoresSafeArea()
            switch page {
            case .artwork:
                ArtworkView(model: model, showFavourites: { showingFavourites = true })
            case .settings:
                SettingsView(shown: model.shown, showGuide: { showingGuide = true })
            }
        }
        .safeAreaInset(edge: .bottom) {
            Picker("Page", selection: $page) {
                ForEach(Page.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            .frame(width: 220)
            .padding(.vertical, 8)
        }
        .sheet(isPresented: $showingGuide, onDismiss: { Preferences.shared.hasSeenAutomationGuide = true }) {
            AutomationGuide()
        }
        .fullScreenCover(isPresented: $showingFavourites, onDismiss: model.reload) {
            FavouritesView()
        }
        .onChange(of: scenePhase) { _, phase in
            switch phase {
            case .active: model.turnIfDue()
            case .background: RefreshSchedule.schedule()
            default: break
            }
        }
        // A fetch that ends on its own (background prefetch, the intent) changes what is shown.
        .onChange(of: RotationStatus.shared.isFetching) { _, _ in model.reload() }
    }
}
