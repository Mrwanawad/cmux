@testable import CmuxNextBrowser
import Foundation
import Testing

/// #55104: after cmux finishes its own GitHub completion link in one of its tabs, the tab closes only when it showed
/// nothing but the connect round trip (GitHub and cmux's console).
@MainActor struct BrowserAppLinksTests {
    @Test func closesOnlyAConnectOnlyTab() throws {
        let gh = try #require(URL(string: "https://github.com/apps/cmux-cloud/installations/new"))
        let console = try #require(URL(string: "https://console.cmux.dev/integrations/callback?state=a.b.c"))
        let other = try #require(URL(string: "https://news.ycombinator.com/"))
        #expect(BrowserAppLinks.onlyConnectPages([gh, console]))
        #expect(!BrowserAppLinks.onlyConnectPages([other, gh, console]))
        #expect(!BrowserAppLinks.onlyConnectPages([]))
        #expect(!BrowserAppLinks.onlyConnectPages([try #require(URL(string: "https://github.com.evil.example/x"))]))
    }

    @Test func withNoHandlerEveryLinkIsIgnored() throws {
        BrowserAppLinks.handle = nil
        #expect(BrowserAppLinks.answer(try #require(URL(string: "cmux-nightly://integration/complete?state=a.b.c"))) == .ignored)
    }
}
