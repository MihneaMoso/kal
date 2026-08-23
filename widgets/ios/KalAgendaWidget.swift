// Kal — iOS WidgetKit agenda widget.
//
// The Rust core is linked as a static library (XCFramework built from
// crates/kal-ffi with `cargo build --target aarch64-apple-ios --release`).
// The timeline provider re-renders up to 5 snapshots of upcoming items.

import WidgetKit
import SwiftUI

// Provided by the kal-ffi XCFramework (see widgets/kal_ffi.h).
typealias KalDbHandle = OpaquePointer

func kal_open(_ path: UnsafePointer<CChar>) -> KalDbHandle?
func kal_close(_ db: UnsafeMutablePointer<KalDbHandle?>)
func kal_upcoming_json(_ db: KalDbHandle?, _ fromEpoch: Int64, _ toEpoch: Int64) -> UnsafeMutablePointer<CChar>?
func kal_free(_ s: UnsafeMutablePointer<CChar>?)

struct KalEntry: TimelineEntry {
    let date: Date
    let items: [KalItem]
}

struct KalItem: Identifiable {
    let id: String
    let title: String
    let start: Date?
    let allDay: Bool
    let kind: String
    let colorHex: String
    let age: Int?

    static func from(json: [String: Any]) -> KalItem? {
        guard let id = json["id"] as? String,
              let title = json["title"] as? String else { return nil }
        let start = (json["start"] as? String).flatMap { ISO8601DateFormatter().date(from: $0) }
        return KalItem(id: id, title: title, start: start,
                       allDay: json["allDay"] as? Bool ?? false,
                       kind: json["kind"] as? String ?? "event",
                       colorHex: json["color"] as? String ?? "#3366cc",
                       age: json["age"] as? Int)
    }
}

struct KalProvider: TimelineProvider {
    private func loadItems(limit: Int) -> [KalItem] {
        guard
            let docs = FileManager.default.urls(for: .libraryDirectory, in: .userDomainMask).first?
                .appendingPathComponent("Database").path,
            let cPath = (docs + "/kal/calendar.db").cString(using: .utf8),
            let handle = kal_open(cPath)
        else { return [] }

        defer { var h: KalDbHandle? = handle; kal_close(&h) }

        let now = Int64(Date().timeIntervalSince1970)
        guard let ptr = kal_upcoming_json(handle, now, now + 14 * 86_400) else { return [] }
        defer { kal_free(ptr) }
        let data = Data(bytes: ptr, count: strlen(ptr))
        guard let array = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else {
            return []
        }
        return array.compactMap(KalItem.from(json:)).prefix(limit).map { $0 }
    }

    func placeholder(in context: Context) -> KalEntry { KalEntry(date: .now, items: []) }

    func getSnapshot(in context: Context, completion: @escaping (KalEntry) -> Void) {
        completion(KalEntry(date: .now, items: loadItems(limit: 8)))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<KalEntry>) -> Void) {
        // Refresh every hour; iOS also wakes us on significant changes.
        let entry = KalEntry(date: .now, items: loadItems(limit: 8))
        completion(Timeline(entries: [entry], policy: .after(.now.addingTimeInterval(3600))))
    }
}

struct KalAgendaView: View {
    let entry: KalEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Kal").font(.caption.bold()).foregroundStyle(.secondary)
            if entry.items.isEmpty {
                Text("Nothing coming up").font(.caption2)
            } else {
                ForEach(entry.items.prefix(widgetFamilyRows)) { item in
                    HStack(spacing: 6) {
                        RoundedRectangle(cornerRadius: 2)
                            .fill(Color(hex: item.colorHex))
                            .frame(width: 4)
                        VStack(alignment: .leading, spacing: 1) {
                            HStack(spacing: 4) {
                                Text(item.title).font(.footnote).lineLimit(1)
                                if item.kind == "birthday", let age = item.age {
                                    Text("\(age)").font(.caption2.bold())
                                        .padding(.horizontal, 4).background(Capsule().opacity(0.15))
                                }
                            }
                            Text(item.allDay ? "all-day" : "\(item.start!, formatter: timeFmt)")
                                .font(.caption2).foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
        .containerBackgroundCompat()
    }

    private var widgetFamilyRows: Int { 8 }
}

private let timeFmt = DateFormatter.localizedFormatter(withTemplate: "HHmm")

extension Color {
    init(hex: String) {
        self.init(hex.hasPrefix("#") ? Color(uiColor: UIColor(hexString: hex)) : .accent)
    }
}

@main
struct KalWidgetsBundle: WidgetBundle {
    var body: some Widget {
        KalAgendaWidget()
    }
}

struct KalAgendaWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "KalAgenda", provider: KalProvider()) { entry in
            KalAgendaView(entry: entry)
        }
        .configurationDisplayName("Kal Agenda")
        .description("Your upcoming events, tasks and birthdays.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge])
    }
}
