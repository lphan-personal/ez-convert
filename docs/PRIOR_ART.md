# Prior art — Filee

Status: **reference implementation only.** Authority for this file is the stakeholder's
standing policy (Ryo, 2026-11-01). It is reproduced here so the repo, not a prompt, is the
source of truth.

Filee (`github.com/KnifeLemon/Filee`, MIT, ~19.1k stars) is a shipped file converter:
Windows-first, C# / .NET 10 / Avalonia 12, Ctrl+drag from Explorer onto a "donut" toolbar at
the cursor. Engines: Magick.NET, PDFsharp, PDFium, rhwp (HWP<->HWPX), optional LibreOffice
and Pandoc downloaded on demand with pinned SHA-256. It ships a real CLI
(`formats --json`, `convert --json -o`) and publishes an Agent Skill. 1,629 commits,
30 releases, 53 contributors.

## Our position

EZ Convert is **Tauri 2 + Rust**. That is a deliberate divergence, **not a port**. Filee is
a **REFERENCE IMPLEMENTATION observed as a black box**. It is never our source, never our
dependency, never our spec. We are not building "Filee for Mac" — we are building a
cross-platform, agent-first converter that overlaps on **problem**, not on **code**.

## What we take from it (ideas only, no code)

- Format-pair taxonomy, and which pairs actually hurt.
- On-demand engine download with pinned SHA-256 verification.
- Per-file-type profiles / presets, route planning, job-queue concepts.
- Quality reporting: Filee's own skill says judge each file by its **state**, not by the
  exit code alone. Adopted in our QA design (see `SPEC.md` §8 and card Q3).
- Their issue tracker as free research. **Read issues, not code.**

## Hard rules (R1-R6) — violation is a BLOCKED card, not a bug

| # | Rule |
|---|------|
| R1 | Filee source is **NEVER** cloned, vendored, submoduled, cached, or placed in any workspace or LLM prompt. Reading README/docs/issues/releases in a browser is allowed and encouraged. Its released **binary** may be run as an oracle for behaviour and output quality. |
| R2 | No C#, no Avalonia, no .NET, no porting of Filee code or file structure. Stack is Rust + Tauri per `SPEC.md`. |
| R3 | No Filee name, icon, logo, screenshots, or doc text in our repo, app, store listing, or any written copy. No wording implying endorsement, partnership, or affiliation. |
| R4 | No novelty claims ("first", "only", "unique", "best"). If prior art is relevant to a feature, **name it**. |
| R5 | Never paste third-party source into an LLM prompt. Cite in one line instead. |
| R6 | Licenses are additive, never silent: every new dependency lands in `docs/LICENSES.md` the moment it is added. LGPL engines (ffmpeg) stay separate-process sidecars — no static link, no bundling into the app binary. MIT/BSD notices retained verbatim. |

## Reconciliation note (R3 vs this file)

R3 forbids the Filee name in "our repo". Naming prior art is also required in this file by
the stakeholder's instruction of 2026-11-01 (§4.1) and permitted by R4. Resolution, adopted
by the engineering lead and flagged to the stakeholder:

- `docs/PRIOR_ART.md` and `docs/REFERENCE_POLICY.md` are the **only** files allowed to name
  Filee / KnifeLemon / filee.sh.
- Every other file, string, asset, screenshot and store listing must have **zero** hits.
- The cross-check deny-list below is scoped accordingly: hits inside these two files are
  expected; hits anywhere else are a violation.
