#!/usr/bin/env bash
set -euo pipefail

MATRIX_FILE="docs/spec/requirements_traceability.md"
STRICT_PROPOSED="${STRICT_PROPOSED:-0}"
EXEMPTIONS_FILE=".traceability_lint_exemptions.txt"
ERRORS=0
WARNINGS=0
CODE_ROOTS=()

for root in src crates; do
  if [[ -d "$root" ]]; then
    CODE_ROOTS+=("$root")
  fi
done

if [[ ${#CODE_ROOTS[@]} -eq 0 ]]; then
  printf '%s:%s:%s: %s: %s\n' "$MATRIX_FILE" "1" "1" "error" "No code roots found to scan (expected src/ or crates/)"
  exit 1
fi

if [[ ! -f "$MATRIX_FILE" ]]; then
  printf '%s:%s:%s: %s: %s\n' "$MATRIX_FILE" "1" "1" "error" "Missing matrix file: $MATRIX_FILE"
  exit 1
fi

if command -v rg >/dev/null 2>&1; then
  SEARCH_TOOL="rg"
else
  SEARCH_TOOL="grep"
fi

trim() {
  local s="$1"
  s="${s#${s%%[![:space:]]*}}"
  s="${s%${s##*[![:space:]]}}"
  printf '%s' "$s"
}

emit_issue() {
  local file="$1"
  local line="$2"
  local col="$3"
  local severity="$4"
  local message="$5"
  printf '%s:%s:%s: %s: %s\n' "$file" "$line" "$col" "$severity" "$message"
}

mapfile -t lifecycle_lines < <(grep -nE '^\| R-[0-9]{2}-[0-9]{3} \| (PROPOSED|IMPLEMENTED|VERIFIED|DEPRECATED) \|' "$MATRIX_FILE" || true)

if [[ ${#lifecycle_lines[@]} -eq 0 ]]; then
  emit_issue "$MATRIX_FILE" "1" "1" "error" "No lifecycle register entries found in $MATRIX_FILE"
  exit 1
fi

missing_required=()
missing_proposed=()
all_matrix_ids=()
exempt_ids=()
declare -A matrix_line_by_id=()

if [[ -f "$EXEMPTIONS_FILE" ]]; then
  mapfile -t exempt_ids < <(grep -E '^R-[0-9]{2}-[0-9]{3}$' "$EXEMPTIONS_FILE" || true)
fi

for line in "${lifecycle_lines[@]}"; do
  line_no="${line%%:*}"
  content="${line#*:}"
  id=$(trim "$(awk -F'|' '{print $2}' <<< "$content")")
  status=$(trim "$(awk -F'|' '{print $3}' <<< "$content")")
  all_matrix_ids+=("$id")
  matrix_line_by_id["$id"]="$line_no"

  if [[ "$status" == "DEPRECATED" ]]; then
    continue
  fi

  if [[ "$SEARCH_TOOL" == "rg" ]]; then
    if rg -n --glob '*.rs' "\[$id\]" "${CODE_ROOTS[@]}" >/dev/null 2>&1; then
      continue
    fi
  else
    if grep -Rns --include='*.rs' "\[$id\]" "${CODE_ROOTS[@]}" >/dev/null 2>&1; then
      continue
    fi
  fi

  if [[ "$status" == "PROPOSED" ]]; then
    missing_proposed+=("$id")
  else
    is_exempt=0
    for eid in "${exempt_ids[@]:-}"; do
      if [[ "$id" == "$eid" ]]; then
        is_exempt=1
        break
      fi
    done

    if [[ $is_exempt -eq 0 ]]; then
      missing_required+=("$id ($status)")
    fi
  fi
done

unknown_anchor_ids=()
declare -A anchor_file_by_id=()
declare -A anchor_line_by_id=()
if [[ "$SEARCH_TOOL" == "rg" ]]; then
  while IFS=: read -r file line token; do
    id="${token#[}"
    id="${id%]}"
    if [[ -z "${anchor_file_by_id[$id]:-}" ]]; then
      anchor_file_by_id["$id"]="$file"
      anchor_line_by_id["$id"]="$line"
    fi
  done < <(rg -n -o --color never --glob '*.rs' '\[R-[0-9]{2}-[0-9]{3}\]' "${CODE_ROOTS[@]}" 2>/dev/null || true)

  mapfile -t anchor_ids < <(printf '%s\n' "${!anchor_file_by_id[@]}" | sort -u)
else
  while IFS=: read -r file line token; do
    id="${token#[}"
    id="${id%]}"
    if [[ -z "${anchor_file_by_id[$id]:-}" ]]; then
      anchor_file_by_id["$id"]="$file"
      anchor_line_by_id["$id"]="$line"
    fi
  done < <(grep -Rhn --include='*.rs' '\[R-[0-9]\{2\}-[0-9]\{3\}\]' "${CODE_ROOTS[@]}" 2>/dev/null || true)

  mapfile -t anchor_ids < <(printf '%s\n' "${!anchor_file_by_id[@]}" | sort -u)
fi

for aid in "${anchor_ids[@]:-}"; do
  found=0
  for mid in "${all_matrix_ids[@]}"; do
    if [[ "$aid" == "$mid" ]]; then
      found=1
      break
    fi
  done
  if [[ $found -eq 0 ]]; then
    unknown_anchor_ids+=("$aid")
  fi
done

if [[ ${#missing_required[@]} -gt 0 ]]; then
  for item in "${missing_required[@]}"; do
    id="${item%% *}"
    line_no="${matrix_line_by_id[$id]:-1}"
    emit_issue "$MATRIX_FILE" "$line_no" "1" "error" "Missing code anchor for $item"
    ERRORS=$((ERRORS + 1))
  done
fi

if [[ ${#unknown_anchor_ids[@]} -gt 0 ]]; then
  for item in "${unknown_anchor_ids[@]}"; do
    file="${anchor_file_by_id[$item]:-$MATRIX_FILE}"
    line_no="${anchor_line_by_id[$item]:-1}"
    emit_issue "$file" "$line_no" "1" "error" "Code anchor $item is not present in matrix lifecycle register"
    ERRORS=$((ERRORS + 1))
  done
fi

if [[ ${#missing_proposed[@]} -gt 0 ]]; then
  if [[ "$STRICT_PROPOSED" == "1" ]]; then
    for item in "${missing_proposed[@]}"; do
      id="${item%% *}"
      line_no="${matrix_line_by_id[$id]:-1}"
      emit_issue "$MATRIX_FILE" "$line_no" "1" "error" "STRICT_PROPOSED=1 and missing code anchor for $item"
      ERRORS=$((ERRORS + 1))
    done
  else
    for item in "${missing_proposed[@]}"; do
      id="${item%% *}"
      line_no="${matrix_line_by_id[$id]:-1}"
      emit_issue "$MATRIX_FILE" "$line_no" "1" "warning" "PROPOSED requirement without code anchor: $item"
      WARNINGS=$((WARNINGS + 1))
    done
  fi
fi

if [[ $ERRORS -gt 0 ]]; then
  exit 1
fi

echo "Traceability lint passed with $WARNINGS warning(s)."
