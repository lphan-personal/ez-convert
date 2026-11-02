# Distribution & signing — DECIDED (2026-11-02)

**Stakeholder decision: EZ Convert is INTERNAL-ONLY. v1 is never published, never
notarized, never sold.**

## What this means for the build

- **macOS:** ad-hoc `codesign` only. On Apple Silicon an ad-hoc signature is *mandatory*
  (an unsigned arm64 binary will not execute), so sign inside-out: the engine pack first,
  then `Contents/MacOS/*`, then the bundle. **No Developer ID, no notarization.**
- **Windows:** unsigned NSIS per-user installer. No Authenticode, no EV cert.
- **No store listings**, no auto-update feed, no Azure Trusted Signing.

## Out of scope — do not build or investigate

Apple Developer Program ($99/yr), notarization, Azure Artifact Signing (~$9.99/mo),
Authenticode/EV certificates, Microsoft Store / Mac App Store packaging, Sparkle or any
update channel. Estimated at ~$219/yr and explicitly declined.

## The one consequence to document

Gatekeeper will quarantine a downloaded DMG. First launch requires
right-click → **Open**, or `xattr -d com.apple.quarantine /Applications/EZ\ Convert.app`.
Windows SmartScreen will show "unrecognised app" → **More info → Run anyway**.
Both belong in the user guide (P4), stated plainly, not buried.

## Revisit condition

Only if the stakeholder decides to publish. Then the order is: fund Developer ID →
sign engine pack → notarize the DMG → staple → document the change.
