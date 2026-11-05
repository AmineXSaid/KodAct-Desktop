---
name: "KodAct"
tagline: "Code, then act, on the model you choose."
specVersion: "0.3.0"
version: 1
language: en
type: master
architecture: branded-house
---

# KodAct

## Strategy

### Overview

KodAct is an agent harness for software work: KodAct CLI (`kodact`) runs the
agent, and KodAct Desktop is its native app. It began as Forge and ForgeCLI,
a personal tool built to work with a coding agent on any model, and was
renamed KodAct in 2026.

What it really does: it lets one developer hand real work to an agent on the
model and endpoint they choose, watch every step it takes (reads, edits,
commands, questions, permissions), steer it while it works, and keep the keys,
the calls and the files on their own machine or on the remote one they work in.

The structural problem: the polished agents are tied to one provider, and the
model-agnostic ones trade polish for choice. Before KodAct, a developer picks
between a first-party tool that locks them in and an open-source one that
feels like a compromise. After KodAct, they get a first-party-grade tool on
whatever model they pick, and they can see and trust what it does.

Ambition: the agent tool a developer keeps when they change models.

### Audience

- **Primary:** solo and independent developers who work with a coding agent
  on their own projects, on Windows, in WSL, over SSH or in a dev container.
  They run long sessions, often several chats at once, and watch the agent
  work rather than only read its answer.
- **What they already believe:** agents are useful now; the model market moves
  every month; their keys and code should stay theirs.
- **What they are skeptical of:** lock-in to one provider, "AI magic" claims,
  tools that hide what the agent did, and open-source wrappers that feel
  unfinished.
- **Secondary:** the same people using KodAct for plain chats and learning (a
  topic over many chats, notes, diagrams, documents). Same person, calmer
  task; the brand does not change for them.
- **Not for:** buyers looking for an enterprise AI platform with seats and
  dashboards; people who want the agent to work unseen and unchecked; anyone
  looking for an AI that "replaces developers".

### Positioning

- **Category:** a model-agnostic agent harness with a native desktop app, for
  software work.
- **Not** a model and not an AI lab. **Not** an IDE or an editor plugin. **Not**
  a cloud platform or a SaaS seat. **Not** a chatbot.
- **Landscape:** first-party agents (Claude Code, Codex, Gemini CLI) are
  polished and tied to their maker's models. Model-agnostic open-source agents
  (OpenCode, Goose, Aider) offer choice and give up polish. IDE agents
  (Cursor, Cline) live inside an editor. KodAct sits where choice and polish
  meet.
- **Structural differentials:**
  - Any model on any endpoint, swappable per chat.
  - Keys and model calls stay on this computer; a remote agent runs on WSL,
    SSH or a container while the app stays local.
  - A native app (Rust), fast, Windows first.
  - Every step the agent takes is shown truthfully, and you can steer it
    mid-turn.
  - Projects carry their own instructions, knowledge and memory.
- **Territory:** the operator's console. KodAct owns the moment between
  "here is the task" and "it is done": you hand it over, and you stay in
  control while it acts.

### Personality

- **Archetype:** The Operator, calm and in control.
- **Attributes:** precision, control, candour, speed, restraint.
- **What it is:** Calm. Exact. Quick. Plain-spoken. Trustworthy.
- **What it is not:** Not hype. Not a hacker costume. Not corporate. Not cute.
  Not magic.

### References & Anti-References

#### References

- **Claude:** Borrow its single ownable mark and the calm, plain confidence of
  its words. Do not borrow its spark, its cream and terracotta, or its type.
- **Linear:** Borrow its precision, restraint and speed you can feel. Do not
  borrow its dark gradients, glow or category vocabulary.
- **Teenage Engineering:** Borrow tools treated as objects, industrial honesty
  and one bright colour on a neutral ground. Do not borrow its toy-like
  playfulness.
- **ArtCraft (app family icons):** Borrow the family lockup (the brand above,
  the icon, the product below) and one saturated tile per mark. Do not borrow
  its animal illustrations.

#### Anti-References

- **AI hype:** reject sparkles, glowing gradients, "revolutionary", "10x",
  "magic" and rockets.
- **Hacker costume:** reject green-on-black terminal cosplay, skulls, "root
  access" and leetspeak.
- **Enterprise SaaS:** reject stock blue, "unlock your potential", "book a
  demo" and interchangeable dashboards.
- **Media player:** reject play, pause and skip shapes; KodAct is work, not
  playback.
- **Generic dark dev tool:** reject the purple-blue glow, the light beam and
  the grid on near-black that every AI developer tool shipped this year.

### Promise

- Any model. No lock-in.
- First-party polish, without the first party.
- You see every step, and you can stop or steer it.
- Your keys and your code stay yours.

**Base message:** KodAct gives you a first-class agent on whatever model you
choose, and keeps you in control while it works.

**Synthesizing phrase:** Your model, your machine, your call.

### Guardrails

- **Tone:** calm, exact, plain, confident, human.
- **KodAct cannot be:** a hype machine, a hacker costume, an enterprise
  platform, a model vendor, a toy, or a black box.
- **Litmus test:** if it would fit any AI startup's landing page, it is not
  KodAct.

## Voice

### Identity

We are KodAct. We are the agent you run on your terms: the model you pick, on
the endpoint you trust, with the keys on your machine. You write the task; we
read, edit and run what it needs, and we show you every step while we do.

We do not pretend the model is magic, and we do not hide what we did. A failed
command says it failed. A stopped call says its outcome is unknown. You can
interrupt, steer and continue at any point, because the work is yours.

We are built to be fast and to stay out of the way: a native app and a CLI
that share one identity, so moving between them feels like one tool.

**Essence:** You decide; KodAct acts, in plain sight.

### Tagline & Slogans

- **Primary:** "Code, then act, on the model you choose." (the CLI's front
  page, About, the README, the site's first line)
- **Alternatives:**
  - "Any model. Your machine. In plain sight."
  - "First-party polish, on any model."
  - "You decide. It acts."
- **Slogans:**
  - "Change models, not tools." (model choice)
  - "Your keys never leave." (privacy)
  - "Watch it work." (transparency)
  - "Hand it the task. Keep the wheel." (control)
  - "Remote work, local control." (WSL, SSH, containers)

### Message Pillars

- **Choice:** Any model on any endpoint, swappable per chat. Change models,
  not tools.
- **Control:** You see every step and can stop or steer it mid-turn. Nothing
  changes without the permission you set.
- **Privacy:** Keys and model calls stay on your computer; a remote agent runs
  where your code lives.
- **Craft:** A native, fast app with first-party polish. The reply is the
  hero; everything else stays quiet.
- **Truth:** It says what happened, in words. A failure is a failure, not a
  spinner.

### Phrases

- "Change models, not tools."
- "Code, then act."
- "First-party polish, without the first party."
- "Hand it the task. Keep the wheel."
- "It shows its work."
- "Your keys never leave this machine."
- "Remote agent, local control."

### Vocabulary

| Concept | We call it | Not |
|---|---|---|
| A conversation with the agent | chat | session, thread, ticket |
| What the agent says back | reply | response, output, completion |
| Where the model runs | endpoint | provider, backend, API |
| How much it may do unasked | mode (Manual, Edit automatically, Plan, Bypass, Expert) | autonomy level, permissions tier |
| A step the agent takes | step, call | action item, operation |
| A folder of work with its memory | project | workspace, repo, space |

Capitalization: **KodAct** (capital K and A, one word), **KodAct CLI**,
**KodAct Desktop**. Never "Kodact", "KODACT" in running text (capitals only
in the lockup), or "Kod Act".

### Tonal Rules

1. Say what happened, in plain words, before saying anything else.
2. Name the problem and the way out; never just the problem.
3. Controls name their action ("Allow", "Stop", "Continue"), not a feeling.
4. Short sentences. One idea each.
5. No exclamation marks in product copy.
6. Never promise what the model will do; describe what KodAct does.
7. Use the product's own terms (see Vocabulary), every time.
8. Confidence without superlatives: no "best", "ultimate", "revolutionary".
9. Talk to one developer, as "you".
10. Admit limits plainly ("Couldn't reach the endpoint. Check the URL in
    Settings → Models.").

**What we are not:**
- We are not magic.
- We are not a model.
- We are not here to replace you.
- We are not hiding anything we did.
- We are not a platform you rent.

| We Say | We Never Say |
|---|---|
| "Change models, not tools." | "The ultimate AI coding platform" |
| "Couldn't write Cargo.toml" | "Oops! Something went wrong" |
| "Runs on the endpoint you choose" | "Powered by cutting-edge AI" |
| "You can stop it at any step" | "Fully autonomous" |
| "Your keys stay on this computer" | "Enterprise-grade security" |
| "It shows its work" | "Magic" |

## Visual

### Logo & Marks

**In development.** The previous mark (a cursor bar beside a run triangle)
is retired: it reads as a media player's pause and play. A new mark is being
chosen in `design/logo/`; this section is completed when one is approved.

Invariants already decided:
- One simple, ownable mark, one colour, legible at 16px.
- Approved lockup: **KODACT** in spaced capitals above the icon, the product
  name (KodAct Desktop, KodAct CLI) below. One mark for both products.
- Never: play, pause or skip shapes; a letter K (too close to Kimi); `>_` (too
  close to PowerShell and every terminal); sparkles, glow, gradients or
  effects; recolouring outside the core colours.

### Core Colors

- **Lime** `#c6ff3d` (mandatory). KodAct acting: the one point of energy. It
  means something because it is rare. Always set against ink, never as a wash.
- **Ink** `#101214` (mandatory). The ground KodAct works on: calm, permanent,
  never pure black.
- **Paper** `#f6f5f1` (optional). The daylight ground, a warm off-white for when
  ink would be too heavy.

Avoid: purple and blue gradients (the generic AI look); stock SaaS blue; the
terminal greens of hacker costume; Claude's cream and terracotta.

### Typefaces

- **KodAct Sans** (KodAct's cut of Host Grotesk), the brand voice: every word.
  Fallback: `"Segoe UI", system-ui, sans-serif`. Licensing: SIL Open Font
  License 1.1 (`assets/fonts/OFL-HostGrotesk.txt`). Verified.
- **KodAct Mono** (KodAct's cut of JetBrains Mono), code, paths and keys only.
  Fallback: `"Cascadia Mono", ui-monospace, monospace`. Licensing: SIL Open
  Font License 1.1 (`assets/fonts/OFL-JetBrainsMono.txt`). Verified.
- **The pixel letters** (KodAct CLI's own glyphs), the name only, at brand
  moments. Not a font.

### Art Direction

- **Keywords:** operator's console, instrument, precision, one signal colour,
  plain sight, restraint, object-like.
- **Direction:** KodAct should look like a well-made instrument you control,
  not a show about AI.
- **Visual territory:** the control panel of a precise machine at night: a
  matte ink surface, one lime indicator that is on when KodAct acts, labels
  in plain type, nothing decorative.

## Governance

### Naming

- Products: **KodAct** + plain noun (KodAct CLI, KodAct Desktop). No invented
  suffixes ("KodAct Pro AI").
- Features are named for what they do, in sentence case ("Terminal dock",
  "Side panel"), never with "AI" or "smart".

### Claims

No approved public claims yet. Draft candidates, not approved:
- (draft) "Runs on any OpenAI-compatible or Anthropic-compatible endpoint."
  Needs checking against the CLI's supported endpoint list.
- (draft) "Keys never leave your computer." Needs a review of every network
  path, including remote work.

Prohibited: speed or quality comparisons with named competitors; user counts;
"autonomous"; any claim about what a model can do.

### Accessibility Commitments

- Text holds at least 4.5:1 against its ground in every theme (checked in the
  desktop app by `src/palette_check.rs`).
- Every action is reachable from the keyboard, with visible focus.
- Reduced motion is honoured everywhere.
