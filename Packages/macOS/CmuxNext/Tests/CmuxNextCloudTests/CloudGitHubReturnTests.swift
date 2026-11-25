@testable import CmuxNextCloud
import Foundation
import Testing

/// #55104: GitHub's return reaches the app whichever browser opened it. The console's page for a settings save with
/// no state is recognised (an in-app tab reads GitHub again), and a return to cmux soon after a connect opened GitHub
/// reads GitHub again, once.
@MainActor struct CloudGitHubReturnTests {
    @Test func recognisesTheConsolesStatelessSettingsSave() throws {
        let saved = try #require(URL(string: "https://console.cmux.dev/integrations/callback?code=956f&installation_id=183460287&setup_action=update"))
        #expect(CloudGitHubCompletion.isSavedOnGitHub(saved))
        for other in [
            "https://console.cmux.dev/integrations/callback?state=a.b.c&installation_id=1&setup_action=update", // a connect: finished elsewhere
            "https://console.cmux.dev/integrations/callback?installation_id=1&setup_action=install",
            "https://console.cmux.dev/integrations/callback?installation_id=1&setup_action=update&error=access_denied",
            "https://console.cmux.dev/integrations/callback?installation_id=x1&setup_action=update",
            "https://evil.example/integrations/callback?installation_id=1&setup_action=update",
            "http://console.cmux.dev/integrations/callback?installation_id=1&setup_action=update",
            "https://console.cmux.dev/integrations?installation_id=1&setup_action=update"
        ] {
            #expect(!CloudGitHubCompletion.isSavedOnGitHub(try #require(URL(string: other))), "\(other)")
        }
    }

    @Test func aReturnSoonAfterOpeningGitHubReadsItAgainOnce() {
        let changes = CloudGitHubChanges()
        let opened = Date(timeIntervalSince1970: 1_000_000)
        #expect(!changes.returned(now: opened)) // nothing opened: no read
        changes.noteOpened(now: opened)
        #expect(changes.returned(now: opened.addingTimeInterval(120)))
        #expect(changes.count == 1)
        #expect(!changes.returned(now: opened.addingTimeInterval(130))) // once per open
        changes.noteOpened(now: opened)
        #expect(!changes.returned(now: opened.addingTimeInterval(CloudGitHubChanges.returnWindow + 1))) // too late
        #expect(changes.count == 1)
    }

    @Test func aCompletionEndsThePendingReturn() {
        let changes = CloudGitHubChanges()
        changes.noteOpened()
        changes.record(.connected, reason: nil)
        #expect(changes.count == 1)
        #expect(!changes.returned())
        #expect(changes.count == 1)
    }
}
