# Design

How the Reach app looks and why. The tokens at the top of `app/src/styles.css` are the source of truth; this page explains them.

## Principles

- **Quiet by default.** White and very light grey surfaces (a deep neutral in dark), hairlines and soft shadows, one accent colour. The task, the steps and the screen are the content.
- **Everything the agent does is visible.** Steps, the screen after each step, and every approval stay in the conversation.
- **Light and dark follow Windows**, unless you pick one in Settings.

## Tokens

| | Values |
|---|---|
| Spacing | 4 px grid |
| Type scale | 12, 13, 14, 15, 16, 20, 28, 36 px; weights 400, 500, 600, 700. Large headings are bold with tight tracking |
| Radii | 6, 8, 12, 16, 20 px and full (pills) |
| Typeface | Inter Variable (bundled), Cascadia Mono for code |
| Icons | `lucide-react` only, no emoji |
| Accent | One colour, cobalt blue #2563ff. White text on it passes 4.5:1. Used for your messages, the main button, the Live badge, the working status, the selected window and the agent's pointer and tag; everything else is neutral. A lighter tone of the same colour is used for links and focus in dark mode |
| Buttons | Accent pills for the main action; outlined pills for the rest |
| Danger | Red, only for Stop, errors and Deny |
| Waiting | Amber, only for "needs you" |

Text keeps at least 4.5:1 contrast against its background in both themes.

## Layout

- **Frame:** the Windows title bar and the sidebar share one chrome colour; the content sits in an inset panel with a 16 px top-left radius.
- **Sidebar:** about 250 px, collapses to an icon strip (Ctrl+B) and on its own below 1000 px. New task and Search at the top; tasks with a short, verb-first title (at most 40 characters, `app/src/title.ts`) and a status line (what it's doing now, or how it ended and when); a Screens section with a small live picture of each monitor and Idle or Working (a screen with nothing to show isn't listed, and with none the section hides); Settings at the bottom.
- **Column:** 760 px for the conversation and the composer.
- **Composer:** one large rounded box with its controls in the bottom row and a round send button.
- **Start:** a large bold heading, the composer (agent, speed, send), and suggestion chips with the app's own icon.
- **Task:** the conversation on the left, the screen panel on the right. The header has the title, the status, Take over and Stop. The screen panel shows the screen after each step with the agent's pointer where it acted, a Live badge, a scrubber to replay, and a thumbnail of each window the agent worked in (click to enlarge). Below 860 px they become two tabs.
- **App icons** come from the installed apps through Windows (Store apps too) and are cached; nothing third-party ships with Reach, and a neutral glyph stands in when there's none.

## Conversation

- The agent's messages are plain text with Markdown (code, bold, tables). Your messages sit in a solid accent bubble with white text.
- Steps form a timeline: the app's icon, one short sentence, and the app and screen it happened on (the time is in the tooltip). Consecutive steps fold into one row you can open.
- **Approvals** are a full-width card above the composer, not a dialog: a small amber icon, what the agent wants to do, where, the exact content, whether it can be undone, and a crop of the screen with the target outlined. Approve, Edit and Deny sit on one row; no answer in 2 minutes counts as Deny.

## The agent's pointer

The agent has its own pointer on screen, separate from your mouse: a slim classic arrow in the accent colour, about 24 px tall at 100 %, with a 1.5 px white outline and a very soft shadow, drawn as vectors at every scale. A tag in the same colour is attached to the arrow, with white 12.5 px semibold text: what it's doing in a word ("Typing", "Clicking"), the agent's name while it thinks, "Needs your OK", "Paused", "Stopped", a check and "Done" (then it fades), or the error. No dot, timer or icon; the step itself is in the timeline. "Needs your OK" is amber, paused and stopped dim the accent, and only errors are red. Where your own pointer is drawn, it's near-black with a "You" tag. The live view draws the same pointer and tag. When an approval is waiting, the tag grows into a small card with Approve, Edit and Deny. It never takes the focus and never appears at your own pointer. Drawn by `crates/reach-core/src/overlay.rs`.

## Service names

ChatGPT is shown with OpenAI's mark next to its name; Claude is shown by name only. Rules and sources are in [branding.md](branding.md).

## Quality rules

- Every screen has empty, loading and error states.
- Full keyboard support with a visible focus ring.
- Readable at 100, 125 and 150 % scaling, down to 900 x 600 logical px, with no wrapping buttons or clipped text.
- Motion stays under 250 ms, and nothing moves when Windows' animation effects are off (`prefers-reduced-motion`).
- In-app wording follows the voice in `app/prompts/agent_system.md`: plain, short, says what happened and what to do next.
- Never: gradients, glow, glassmorphism, gradient text or emoji.
