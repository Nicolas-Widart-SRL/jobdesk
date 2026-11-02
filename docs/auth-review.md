# Authentication review

Reviewed 31 October 2026 against Anthropic's published documentation. This is an engineering reading of those pages, not legal advice. The terms can change.

## How Jobdesk authenticates

Jobdesk starts the user's installed, unmodified `claude` binary as a child process with `-p`. By default the CLI uses whatever authentication the user already set up, and Jobdesk offers no login and never reads, stores or passes that credential. The one exception is optional: an Anthropic Console API key the user pastes in Settings › Providers. Jobdesk keeps it only in the system keychain, the macOS Keychain or the Windows Credential Manager, and passes it only as `ANTHROPIC_API_KEY` in the environment of each task's `claude` process, never on its command line. Save key tests the key first, with Test connection, and tasks switch to it only after that test passes. A refused key leaves them on the Claude sign-in. Settings › Providers and Settings › Runtime status show whether the last task used a Claude login or an API key, using the CLI's own `apiKeySource` field. The status bar no longer shows it.

## What the documentation says

| Point | Text | Source |
| --- | --- | --- |
| Third-party apps | Developers may not "offer Claude.ai login into their own applications" | [Legal and compliance](https://code.claude.com/docs/en/legal-and-compliance) |
| Routing | Nor may they "route requests through" consumer plan credentials on behalf of users | Legal and compliance |
| Credentials | Developers "may not collect, store, or intermediate" Claude.ai credentials or tokens | Legal and compliance |
| Builders | Developers "should use API key authentication through Claude Console" or a supported cloud provider | Legal and compliance |
| End users | The terms do not stop a user "signing in to the unmodified Claude Code binary" with their own subscription | Legal and compliance |
| Limits | Plan limits "assume ordinary, individual usage" | Legal and compliance |
| Agent SDK | Third parties may not offer claude.ai login or rate limits "unless previously approved" | [Agent SDK overview](https://code.claude.com/docs/en/agent-sdk/overview) |
| Subprocess | The SDK page documents running the CLI "as a subprocess with the -p flag" | Agent SDK overview |
| Scripts | `CLAUDE_CODE_OAUTH_TOKEN` is subscription auth for "CI pipelines and scripts" | [Authentication](https://code.claude.com/docs/en/authentication) |
| Automation | Outside API keys, consumer terms bar access "through automated or non-human means" unless explicitly permitted | [Consumer terms](https://www.anthropic.com/legal/consumer-terms) |
| Naming | Products may say they run Claude Code, but may not use the name in their own product name | Legal and compliance |

No page says whether a third-party app that launches a subscription-authenticated `claude -p` is allowed or forbidden.

## Assessment

**Use on your own machine.** Jobdesk fits the signing-in carve-out and collects no Claude.ai credentials, and the docs describe `-p` scripting. The consumer terms' automation clause still pulls the other way, and a person starts each run but the app drives it. This is unresolved. An API key avoids the question entirely.

**Public release with Claude login.** Do not ship this without written approval. A distributed app that runs on each user's plan comes close to the routing restriction, and the guidance for builders is API keys. Anthropic also reserves the right to enforce without notice.

**Public release with API keys.** This is the documented path. Each user supplies their own Console key or cloud provider credentials, and the Commercial Terms apply. The Commercial Terms and Quickstart were not part of this review, so read them before release.

## Before a public release

1. Choose one route: ask Anthropic for approval, or make API keys the only supported route.
2. Done: for the API key route, the key reaches the CLI from Jobdesk's own key entry in Settings › Providers, stored in the system keychain, because apps opened from the Finder do not inherit shell variables.
3. State in the README how the app authenticates, as the maintainer asked in [discussion 673](https://github.com/ai-job-search-dev/ai-job-search/discussions/673).
4. Keep the name Jobdesk and use no Anthropic or Claude Code logos. Plain text saying it runs Claude Code is allowed.
5. Sign and notarize the macOS build, and sign the Windows installer.
