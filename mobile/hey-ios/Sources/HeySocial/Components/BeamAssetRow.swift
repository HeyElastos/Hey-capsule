import SwiftUI

/// One asset row inside the BEAM sheet — name + optional sub-label + maturing hint
/// on the left, the balance in gold on the right. Port of MainActivity.kt:3792-3801.
///
/// SHARED component (owned by the "beam" group). Reuse it; do not redefine.
struct BeamAssetRow: View {
    @Environment(\.colorScheme) private var scheme
    let name: String
    let balance: String
    var maturing: String? = nil
    var sub: String? = nil

    init(_ name: String, _ balance: String, maturing: String? = nil, sub: String? = nil) {
        self.name = name
        self.balance = balance
        self.maturing = maturing
        self.sub = sub
    }

    var body: some View {
        HStack(alignment: .center) {
            VStack(alignment: .leading, spacing: 2) {
                Text(name)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Hey.ink(scheme))
                if let sub {
                    Text(sub).font(.system(size: 11)).foregroundStyle(Hey.muted(scheme))
                }
                if let maturing, maturing != "0" {
                    Text("+\(maturing) maturing")
                        .font(.system(size: 11))
                        .foregroundStyle(Hey.muted(scheme))
                }
            }
            Spacer(minLength: 8)
            Text(balance)
                .font(.system(size: 18, weight: .bold))
                .foregroundStyle(Hey.goldInk(scheme))
        }
        .frame(maxWidth: .infinity)
        .padding(14)
        .glass(12)
    }
}
