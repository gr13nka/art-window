import SwiftUI

/// iOS will not let an app set the wallpaper or create an automation, so the app can
/// only show the four steps. They are drawn rather than screenshotted, so there are
/// no images to keep in step with iOS's own wording.
struct AutomationGuide: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    Text("iOS only lets Shortcuts change the wallpaper. Set this up once and a new painting arrives every morning.")
                        .foregroundStyle(.secondary)

                    step(1, "Open Shortcuts, tap Automation, then the + button.") {
                        MockRow(items: ["Shortcuts", "Automation", "Gallery"], highlighted: 1)
                    }
                    step(2, "Choose Time of Day. Set 05:05, Repeat Daily, and Run Immediately.") {
                        MockList(rows: [("Time of Day", "05:05"), ("Repeat", "Daily"), ("Run Immediately", "✓")])
                    }
                    step(3, "Add the action Get Today's Painting, from Art Window.") {
                        MockList(rows: [("Get Today's Painting", "Art Window")])
                    }
                    step(4, "Add Set Wallpaper. Choose Lock Screen and Home Screen, then open its arrow and turn off Show Preview.") {
                        MockList(rows: [("Set Wallpaper", "Lock and Home"), ("Show Preview", "Off")])
                    }
                }
                .padding(20)
            }
            .background(Color.wall)
            .navigationTitle("Daily wallpaper")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
            .safeAreaInset(edge: .bottom) {
                Button("Open Shortcuts") { openURL(URL(string: "shortcuts://")!) }
                    .buttonStyle(.borderedProminent).controlSize(.large)
                    .padding(.vertical, 8)
            }
        }
    }

    private func step(_ number: Int, _ text: String, @ViewBuilder mock: () -> some View) -> some View {
        HStack(alignment: .top, spacing: 14) {
            Text("\(number)")
                .font(.headline).foregroundStyle(.black)
                .frame(width: 28, height: 28).background(Circle().fill(Color.accentColor))
            VStack(alignment: .leading, spacing: 10) {
                Text(text)
                mock()
            }
        }
    }
}

private struct MockRow: View {
    let items: [String]
    let highlighted: Int

    var body: some View {
        HStack {
            ForEach(items.indices, id: \.self) { index in
                Text(items[index])
                    .font(.footnote.weight(index == highlighted ? .semibold : .regular))
                    .foregroundStyle(index == highlighted ? Color.accentColor : .secondary)
                    .frame(maxWidth: .infinity)
            }
        }
        .padding(.vertical, 10)
        .background(RoundedRectangle(cornerRadius: 10).fill(Color.white.opacity(0.07)))
    }
}

private struct MockList: View {
    let rows: [(String, String)]

    var body: some View {
        VStack(spacing: 0) {
            ForEach(rows.indices, id: \.self) { index in
                if index > 0 { Divider().overlay(Color.hairline) }
                HStack {
                    Text(rows[index].0)
                    Spacer()
                    Text(rows[index].1).foregroundStyle(.secondary)
                }
                .font(.footnote)
                .padding(.horizontal, 12).padding(.vertical, 10)
            }
        }
        .background(RoundedRectangle(cornerRadius: 10).fill(Color.white.opacity(0.07)))
    }
}
