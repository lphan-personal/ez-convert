#!/usr/bin/env bash
# =============================================================================
# dist-license-gate.sh — shipped-artifact licence gate (Q2, audit condition: an
# inventory table of every shipped binary/file + a CI assertion that fails the
# build if a GPL-contaminated binary appears).
#
# This gate is LICENSE-DERIVED, not name-based. A banned component nobody thought
# of still gets caught, because every shipped file must resolve to a row of
# docs/shipped-inventory.tsv and that row's licence is classified here. The
# historical example: `libjbig-0.dll` (JBIG-KIT 2.1, GPL-2.0) rode into the vips
# dist via libtiff and passed a name-grep gate. An unattributed file in a scanned
# tree is exactly the failure mode this gate exists to stop, so it FAILS CLOSED.
#
# USAGE
#   scripts/dist-license-gate.sh --check-inventory   classify the inventory + crate graph (CI)
#   scripts/dist-license-gate.sh --scan <dir>        attribute+classify every file in a dist tree
#   scripts/dist-license-gate.sh --selftest          prove the gate rejects contamination (CI)
#
# EXIT: 0 clean, 1 gate failure, 2 usage.
#
# CLASSIFICATION (per licence clause; `WITH` binds to the term before it)
#   any AGPL-*                                    -> FAIL, no exceptions.
#   GPL-* on a `binary` row                       -> FAIL unless the row records
#                                                    status `runtime-exception` AND the
#                                                    licence says `WITH gcc-exception-*`,
#                                                    or status `elected:<arm>`.
#   GPL-* on a non-binary row (fonts, data)       -> FAIL unless the row records
#                                                    `elected:<arm>` (we ship a permissive
#                                                    arm) or `decision-needed` (explicitly
#                                                    surfaced; WARN, listed in the report).
#   LGPL-*                                        -> OK (sidecar layout + notices is the
#                                                    whole point; see THIRD-PARTY-NOTICES).
#   everything else (MIT/BSD/Apache/MPL/0BSD/Zlib/Unlicense/OFL/...) -> OK.
#   LicenseRef-unverified on a binary row         -> FAIL; on data -> WARN + listed.
#   rows with status `planned` / `not-shipped`    -> not enforced (nothing ships yet) —
#                                                    and, in --scan, a file that matches
#                                                    such a row FAILS: a decision to drop
#                                                    content is only real once the file is
#                                                    gone from the tree.
#
# --scan additionally:
#   * every file must attribute to an inventory row (basename or path glob), else FAIL;
#   * when the matched row pins a sha256, a mismatch is FAIL;
#   * any `ffmpeg`/`ffprobe` binary whose build config advertises --enable-gpl or
#     --enable-nonfree is FAIL ("enable-gpl is non-zero").

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INVENTORY="${INVENTORY:-${REPO_ROOT}/docs/shipped-inventory.tsv}"
BOLD_GPL_RE='(^|[^a-z0-9])gpl'   # matched after lgpl/agpl tokens are removed

log() { printf '%s\n' "$*"; }

lower() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }

# Strip agpl* and lgpl* tokens so the bare-gpl test cannot see them.
strip_lgpl_agpl() {
  printf '%s' "$1" | sed -E 's/(a|l)gpl[a-z0-9.+-]*//g'
}

has_agpl() { grep -qE '(^|[^a-z0-9])agpl' <<<"$(lower "$1")"; }
has_lgpl() { grep -qE '(^|[^a-z0-9])lgpl' <<<"$(lower "$1")"; }
has_gpl()  { grep -qE "${BOLD_GPL_RE}" <<<"$(lower "$(strip_lgpl_agpl "$1")")"; }
has_gcc_exception() { grep -qE 'gcc-exception' <<<"$(lower "$1")"; }

# classify <kind> <license> <status>  -> sets C_VERDICT="OK" | "WARN <reason>" | "FAIL <reason>"
C_VERDICT=""
classify() {
  local kind="$1" lic="$2" status="$3"
  case "$status" in planned|not-shipped|notice-text) C_VERDICT="OK"; return ;; esac

  if has_agpl "$lic"; then
    C_VERDICT="FAIL AGPL term ($lic)"; return
  fi

  if has_gpl "$lic"; then
    case "$status" in
      elected:*) C_VERDICT="OK"; return ;;
      runtime-exception)
        if has_gcc_exception "$lic"; then C_VERDICT="OK"
        else C_VERDICT="FAIL runtime-exception claimed but no GCC-exception in licence ($lic)"; fi; return ;;
      decision-needed)
        if [ "$kind" = "binary" ]; then
          C_VERDICT="FAIL GPL on a binary cannot be decision-needed ($lic)"
        else
          C_VERDICT="WARN GPL content awaiting keep/strip decision ($lic)"
        fi; return ;;
      *)
        C_VERDICT="FAIL GPL term with no election/exception recorded (kind=$kind status=$status lic=$lic)"; return ;;
    esac
  fi

  if [ "$lic" = "LicenseRef-unverified" ]; then
    if [ "$kind" = "binary" ]; then
      C_VERDICT="FAIL unverified licence on a binary"
    else
      C_VERDICT="WARN unverified licence ($lic)"
    fi; return
  fi

  C_VERDICT="OK"
}

# ------------------------------------------------------------------ mode: inventory
check_inventory() {
  local n=0 fails=0 warns=0 line kind comp ver path lic url sha status notes verdict
  while IFS=$'\t' read -r kind comp ver path lic url sha status notes; do
    case "$kind" in ''|'#'*) continue ;; esac
    n=$((n + 1))
    if [ "$kind" != "app" ] && [ "$kind" != "binary" ] && [ "$kind" != "font" ] \
       && [ "$kind" != "data" ] && [ "$kind" != "notice" ]; then
      log "FAIL row $n ($comp): unknown kind '$kind'"; fails=$((fails + 1)); continue
    fi
    verdict=""
    classify "$kind" "$lic" "$status"
    verdict="$C_VERDICT"
    case "$verdict" in
      OK) ;;
      WARN*) log "WARN ${comp} :: ${verdict#WARN }"; warns=$((warns + 1)) ;;
      FAIL*) log "FAIL ${comp} :: ${verdict#FAIL }"; fails=$((fails + 1)) ;;
    esac
  done < "$INVENTORY"

  # Crate graph (docs/LICENSES.md): the License column of every crate row must be clean.
  if [ -f "${REPO_ROOT}/docs/LICENSES.md" ]; then
    local crate_lic
    while IFS= read -r crate_lic; do
      if has_agpl "$crate_lic"; then
        log "FAIL crate graph: AGPL in '$crate_lic'"; fails=$((fails + 1))
      elif has_gpl "$crate_lic"; then
        log "FAIL crate graph: bare GPL in '$crate_lic'"; fails=$((fails + 1))
      fi
    done < <(sed -nE 's/^\| `[^`]+` \| [^|]+ \| ([^|]+) \|$/\1/p' "${REPO_ROOT}/docs/LICENSES.md")
  else
    log "FAIL docs/LICENSES.md missing"; fails=$((fails + 1))
  fi

  log "inventory: ${n} rows checked, ${fails} failures, ${warns} warnings"
  [ "$fails" -eq 0 ]
}

# ------------------------------------------------------------------ mode: scan
RP_PAT=""
row_pattern() { # the glob in column 4 (strip "win64: "/"linux64: " style prefixes) -> RP_PAT
  local p="$1"
  p="${p#*: }"
  case "$p" in *' ('*) p="${p%% (*}" ;; esac
  RP_PAT="$p"
}

M_KIND=""; M_COMP=""; M_VER=""; M_PAT=""; M_LIC=""; M_URL=""; M_SHA=""; M_STATUS=""; M_NOTES=""
match_row() { # <relpath> -> sets M_* globals for the best (longest-pattern) matching row
  local rel="$1" base pat matched=-1 kind comp ver path lic url sha status notes
  base="${rel##*/}"
  M_KIND=""
  while IFS=$'\t' read -r kind comp ver path lic url sha status notes; do
    case "$kind" in ''|'#'*) continue ;; esac
    row_pattern "$path"
    pat="$RP_PAT"
    local hit=0
    case "$pat" in
      */\*\*) case "$rel" in "${pat%/**}/"*) hit=1 ;; esac ;;
      */\*)   case "$rel" in "${pat%/*}/"*) hit=1 ;; esac ;;
      *\**)   case "$base" in ${pat}) hit=1 ;; esac ;;
      *)      if [ "$pat" = "$base" ] || [ "$pat" = "$rel" ]; then hit=1; fi ;;
    esac
    # A directory-qualified pattern whose LAST segment is a glob — e.g.
    # share/fonts/truetype/DavidCLM-* — must match prefix + basename, otherwise
    # the per-file rows lose to a shorter directory row (`share/*`) and a GPL
    # font file would be attributed to already-covered in-tree content.
    case "$pat" in
      */*\**)
        if [ "$hit" -eq 0 ]; then
          local pfx="${pat%/*}" tll="${pat##*/}"
          case "$rel" in
            "${pfx}/"*) case "$base" in ${tll}) hit=1 ;; esac ;;
          esac
        fi ;;
    esac
    if [ "$hit" -eq 1 ] && [ "${#pat}" -gt "$matched" ]; then
      matched="${#pat}"
      M_KIND="$kind"; M_COMP="$comp"; M_VER="$ver"; M_PAT="$pat"
      M_LIC="$lic"; M_URL="$url"; M_SHA="$sha"; M_STATUS="$status"; M_NOTES="$notes"
    fi
  done < "$INVENTORY"
  [ "$matched" -ge 0 ]
}

scan_tree() {
  local root="$1" fails=0 files=0 rel verdict
  [ -d "$root" ] || { log "usage: --scan <dir>"; exit 2; }
  while IFS= read -r -d '' f; do
    rel="${f#"${root%/}"/}"
    files=$((files + 1))
    if ! match_row "$rel"; then
      log "FAIL unattributed file in dist: $rel (no row in shipped-inventory.tsv)"
      fails=$((fails + 1)); continue
    fi
    case "$M_STATUS" in
      not-shipped|planned)
        log "FAIL $rel :: inventory row '${M_COMP}' is marked ${M_STATUS}, but the file is in the shipped tree — a strip/drop decision is only real once the file is gone"
        fails=$((fails + 1)); continue ;;
    esac
    classify "$M_KIND" "$M_LIC" "$M_STATUS"
    case "$C_VERDICT" in
      OK) ;;
      WARN*) log "WARN $rel :: ${C_VERDICT#WARN }" ;;
      FAIL*) log "FAIL $rel :: ${C_VERDICT#FAIL }"; fails=$((fails + 1)) ;;
    esac
    if [ "$M_SHA" != "-" ] && [ -n "$M_SHA" ]; then
      local actual
      actual="$(sha256sum "$f" | cut -d' ' -f1)"
      # Hex digests are case-insensitive; the inventory rows were written by
      # hand in upper case, `sha256sum` emits lower case.
      if [ "${actual,,}" != "${M_SHA,,}" ]; then
        log "FAIL $rel :: sha256 drift (inventory $M_SHA, on disk $actual)"
        fails=$((fails + 1))
      fi
    fi
    case "${rel##*/}" in
      ffmpeg|ffmpeg.exe|ffprobe|ffprobe.exe|ffmpeg-*|ffprobe-*)
        local cfg=""
        if [ -x "$f" ] && [ "${f%.exe}" = "$f" ]; then
          cfg="$("$f" -version 2>&1 | head -n 8)"
        else
          cfg="$(LC_ALL=C grep -a -o -E -- '--enable-[a-z0-9-]+' "$f" 2>/dev/null | sort -u)"
        fi
        if grep -qE -- '--enable-(gpl|nonfree)' <<<"$cfg"; then
          log "FAIL $rel :: ffmpeg build advertises --enable-gpl/--enable-nonfree — not shippable"
          fails=$((fails + 1))
        fi ;;
    esac
  done < <(find "${root%/}" -type f -print0)
  log "scan: ${files} files attributed, ${fails} failures"
  [ "$fails" -eq 0 ]
}

# ------------------------------------------------------------------ mode: selftest
selftest() {
  local tmp status=0 rc
  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp:-}"' EXIT

  # --- fixtures ---------------------------------------------------------
  mkdir -p "$tmp/tree" "$tmp/plug"
  printf 'clean binary fixture' > "$tmp/tree/good.dll"
  printf 'contaminant fixture'  > "$tmp/tree/evil.dll"
  printf 'fake gpl ffmpeg'      > "$tmp/plug/ffmpeg"
  cat > "$tmp/plug/ffmpeg.ver" <<'EOF'
configuration: --enable-gpl --enable-libx264
EOF

  local good_sha evil_sha
  good_sha="$(sha256sum "$tmp/tree/good.dll" | cut -d' ' -f1)"

  # inventory A: clean
  cat > "$tmp/inv-clean.tsv" <<EOF
# fixture inventory
binary	fixture	1.0	good.dll	MIT	https://example.invalid/good	${good_sha}	shipped	selftest fixture
EOF
  # inventory B: GPL binary row, correctly attributed
  cat > "$tmp/inv-gpl.tsv" <<EOF
binary	fixture	1.0	good.dll	GPL-2.0-only	https://example.invalid/gpl	${good_sha}	shipped	selftest fixture
EOF
  # inventory C: AGPL row
  cat > "$tmp/inv-agpl.tsv" <<EOF
binary	fixture	1.0	good.dll	AGPL-3.0-only	https://example.invalid/agpl	${good_sha}	shipped	selftest fixture
EOF
  # inventory D: clean + ffmpeg row (the scanned file carries --enable-gpl strings)
  cat > "$tmp/inv-ffmpeg.tsv" <<EOF
binary	fixture	1.0	good.dll	MIT	https://example.invalid/good	${good_sha}	shipped	selftest fixture
binary	ffmpeg	1.0	ffmpeg	LGPL-3.0	https://example.invalid/ff	-	shipped	selftest fixture
EOF

  expect_fail() { # <label> <rc>
    if [ "$2" -eq 1 ]; then log "selftest: PASS ($1 rejected)"; else log "selftest: FAIL ($1 was NOT rejected, rc=$2)"; status=1; fi
  }
  expect_pass() { # <label> <rc>
    if [ "$2" -eq 0 ]; then log "selftest: PASS ($1 accepted)"; else log "selftest: FAIL ($1 was NOT accepted, rc=$2)"; status=1; fi
  }

  # 1: clean tree + clean inventory must pass
  rm -f "$tmp/tree/evil.dll"
  INVENTORY="$tmp/inv-clean.tsv" bash "${BASH_SOURCE[0]}" --scan "$tmp/tree" >/dev/null 2>&1
  expect_pass "clean tree" $?

  # 2: an unattributed binary must fail the scan (fail-closed)
  printf 'contaminant fixture' > "$tmp/tree/evil.dll"
  INVENTORY="$tmp/inv-clean.tsv" bash "${BASH_SOURCE[0]}" --scan "$tmp/tree" >/dev/null 2>&1
  expect_fail "unattributed binary (the jbigkit shape)" $?

  # 3: a GPL-contaminated binary row must fail inventory classification
  INVENTORY="$tmp/inv-gpl.tsv" bash "${BASH_SOURCE[0]}" --check-inventory >/dev/null 2>&1
  expect_fail "GPL binary row" $?

  # 4: an AGPL row must fail
  INVENTORY="$tmp/inv-agpl.tsv" bash "${BASH_SOURCE[0]}" --check-inventory >/dev/null 2>&1
  expect_fail "AGPL row" $?

  # 5: a GPL-configured ffmpeg must fail the scan even when attributed
  cat > "$tmp/plug/ffmpeg" <<'EOF'
#!/usr/bin/env bash
echo "ffmpeg version 7.1 Copyright (c) 2000-2024 the FFmpeg developers"
echo "built with gcc 13"
echo "configuration: --prefix=/usr --enable-gpl --enable-libx264"
EOF
  rm -f "$tmp/plug/ffmpeg.ver"
  chmod +x "$tmp/plug/ffmpeg"
  INVENTORY="$tmp/inv-ffmpeg.tsv" bash "${BASH_SOURCE[0]}" --scan "$tmp/plug" >/dev/null 2>&1
  expect_fail "ffmpeg with --enable-gpl" $?

  # 6: a file that matches a `not-shipped` row must fail the scan — an inventory
  #    decision to drop content only counts once the content is actually gone
  rm -f "$tmp/tree/evil.dll"
  cat > "$tmp/inv-strip.tsv" <<EOF
# fixture inventory: one live row + one stripped row
binary	fixture	1.0	good.dll	MIT	https://example.invalid/good	${good_sha}	shipped	selftest fixture
font	fixture-stripped	-	stripped-*	GPL-2.0-only	https://example.invalid/font	-	not-shipped	stripped by decision
EOF
  printf 'stripped font fixture' > "$tmp/tree/stripped-font.ttf"
  INVENTORY="$tmp/inv-strip.tsv" bash "${BASH_SOURCE[0]}" --scan "$tmp/tree" >/dev/null 2>&1
  expect_fail "file matching a not-shipped row" $?
  rm -f "$tmp/tree/stripped-font.ttf"

  # 7: a directory-qualified row whose last segment is a glob must attribute the
  #    file (control), and must beat a shorter broad row when it says not-shipped
  mkdir -p "$tmp/pack/share/fonts/truetype"
  printf 'font fixture' > "$tmp/pack/share/fonts/truetype/DavidCLM-Bold.otf"
  cat > "$tmp/inv-font.tsv" <<EOF
# fixture inventory: only the specific row — attribution proves dir+basename matching
font	fixture-font	-	share/fonts/truetype/DavidCLM-*	OFL-1.1	https://example.invalid/font	-	shipped	selftest fixture
EOF
  INVENTORY="$tmp/inv-font.tsv" bash "${BASH_SOURCE[0]}" --scan "$tmp/pack" >/dev/null 2>&1
  expect_pass "dir-qualified basename glob attributes its file" $?

  cat > "$tmp/inv-font2.tsv" <<EOF
# fixture inventory: the broad row is shipped, the specific one is stripped —
# the specific row must win, so the file has to FAIL
data	fixture-share	-	share/*	MPL-2.0	https://example.invalid/share	-	shipped	selftest fixture
font	fixture-font	-	share/fonts/truetype/DavidCLM-*	GPL-2.0-only	https://example.invalid/font	-	not-shipped	stripped by decision
EOF
  INVENTORY="$tmp/inv-font2.tsv" bash "${BASH_SOURCE[0]}" --scan "$tmp/pack" >/dev/null 2>&1
  expect_fail "specific not-shipped row beats a broad shipped row" $?
  rm -rf "$tmp/pack"

  # 8: the REAL inventory must pass classification
  bash "${BASH_SOURCE[0]}" --check-inventory >/dev/null 2>&1
  expect_pass "real shipped-inventory.tsv" $?

  log "selftest: $([ "$status" -eq 0 ] && echo ALL PASS || echo FAILURES PRESENT)"
  return "$status"
}

case "${1:-}" in
  --check-inventory) check_inventory ;;
  --scan) shift; scan_tree "${1:?usage: --scan <dir>}" ;;
  --selftest) selftest ;;
  *) log "usage: $0 --check-inventory | --scan <dir> | --selftest" >&2; exit 2 ;;
esac
