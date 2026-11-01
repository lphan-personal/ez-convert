# Reference policy — prior art handling + mandatory cross-check

Authority: stakeholder standing policy, Ryo, 2026-11-01. Applies to `wright`, `smith`,
`scout`, `forge`, `scribe`, `warden`, `default` (Aiko), and every future card on the
`ezconvert` board and any successor board.

Read `docs/PRIOR_ART.md` first. R1-R6 are **blocking** rules.

## 1. Mandatory end-of-function cross-check

Every agent, at the end of **every work unit** — before marking a card done, pushing a
commit, or returning a result — appends this block to the card comment. **No exemptions,
not even for trivial changes.** A missing block means the card is **not done**.

```
CROSS-CHECK (policy v1)
- source_consulted: none | docs | issues | binary:<version>
- code_origin: original | derived:<source+license>
- filee_code_present: no (verified: no .cs/.xaml, no vendored tree, no submodule)
- deny_list_hits: none | <matches for "Filee", "KnifeLemon", "filee.sh">
- licenses_added: none | <name+version+license>
- lgpl_boundary: intact (sidecar) | N/A
- claims_check: no novelty language in docs/README/strings
- verdict: PASS | BLOCKED:<reason>
```

If verdict is **BLOCKED**: stop. Do not push, do not mark done, do not work around it.
Comment the block and the exact rule (R1-R6) on the card and hand back.

`warden` independently re-runs this check at the QA gate. A PASS that `warden` overturns is
a **process failure**, not a judgement call.

## 2. Scoping the deny-list

`docs/PRIOR_ART.md` and `docs/REFERENCE_POLICY.md` are the **only** files permitted to name
Filee / KnifeLemon / filee.sh. Everywhere else — source, strings, UI copy, README, assets,
store listings, commit messages — zero hits allowed.

Reference check (a starting point, not a substitute for reading the diff):

```bash
grep -rniE "\bfilee\b|\bknifelemon\b" --exclude-dir=.git --exclude-dir=target \
  . | grep -vE "^\./docs/(PRIOR_ART|REFERENCE_POLICY)\.md:" || echo "clean"
find . -name "*.cs" -o -name "*.xaml" -o -name "*.csproj" | grep -v '^$' && echo "VIOLATION R2"
[ -f .gitmodules ] && echo "VIOLATION R1 (submodule present)"
```

## 3. Escalation rule

Unsure whether something crosses R1-R6? **Treat it as crossed and escalate.** A blocked
card costs an hour. A leaked derivative in a repo that later goes public costs the project.

## 4. What this policy does NOT forbid

- Reading Filee's README, docs, issues, releases, or its published Agent Skill in a browser.
- Running its released **binary** as a behavioural/output-quality oracle.
- Using its observed behaviour to design our own format taxonomy, presets, queue model and
  quality reporting — as ideas, never as code.
- Naming prior art in prose where it is relevant (R4 requires it).

## 5. Amendment log

| Date | Change |
|------|--------|
| 2026-11-01 | Created (stakeholder policy v1). Deny-list scoping and escalation rule recorded by engineering lead. |
