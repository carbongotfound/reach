# Security policy

Reach drives a real Windows desktop on behalf of an AI agent, so security problems can have real effects. Please report them privately.

## How to report
Use GitHub's private vulnerability reporting: open the repository's **Security** tab and choose **Report a vulnerability** ([direct link](https://github.com/carbongotfound/reach/security/advisories/new)). Please don't open a public issue.

Include what you did, what happened and what you expected, the Reach version, and the Windows version. A test page or window that reproduces it helps a lot.

## What counts
- A way to make Reach act without the approval card when it should ask (for example a send or delete that gets past the engine guard). The known gaps listed in [docs/safety.md](docs/safety.md) are already documented, but a new one is in scope.
- A way for another program or web page to call Reach's local server (`127.0.0.1`) without its token.
- Screen text or a web page that makes the agent follow its instructions despite the safety prompt, with a reliable reproduction.
- Stop or Ctrl+Alt+Esc failing to halt an action.
- Reach sending data that [docs/privacy.md](docs/privacy.md) says stays local.

Problems in Claude Code, Codex or the AI models themselves should go to Anthropic or OpenAI.

## Supported versions
Only the latest release gets fixes.
