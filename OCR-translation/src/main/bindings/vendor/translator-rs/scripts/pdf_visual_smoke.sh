#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 1 ]; then
  echo "usage: $0 <pdf> [<pdf> ...]" >&2
  exit 2
fi

for cmd in cargo pdfinfo mutool convert; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "missing required command: $cmd" >&2
    exit 1
  fi
done

repo_root="$(git rev-parse --show-toplevel)"
bucket="${PDF_VISUAL_BUCKET_DIR:-$repo_root/smoke-bucket}"
out_root="${PDF_VISUAL_OUT_DIR:-$repo_root/smoke-out/visual}"
dpi="${PDF_VISUAL_DPI:-144}"
max_pages="${PDF_VISUAL_MAX_PAGES:-6}"
page_from="${PDF_VISUAL_PAGE_FROM:-1}"
page_to="${PDF_VISUAL_PAGE_TO:-}"
target_lang="${PDF_VISUAL_TARGET_LANG:-en}"
source_lang="${PDF_VISUAL_SOURCE_LANG:-en}"

if [ ! -f "$bucket/index.json" ]; then
  echo "bucket index not found: $bucket/index.json" >&2
  exit 1
fi

if [ "$max_pages" -gt 0 ]; then
  for cmd in pdfseparate pdfunite; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
      echo "missing required command for page trimming: $cmd" >&2
      exit 1
    fi
  done
fi

mkdir -p "$out_root"

sanitize_name() {
  local base
  base="$(basename "$1")"
  base="${base%.*}"
  printf '%s' "$base" | tr -cs 'A-Za-z0-9._-' '-'
}

page_count() {
  pdfinfo "$1" 2>/dev/null | awk '/^Pages:/ { print $2; exit }'
}

prepare_input_pdf() {
  local pdf="$1"
  local dir="$2"
  local pages="$3"
  local selected="$pdf"
  local first="$page_from"
  local last="$page_to"

  if [ -n "$last" ]; then
    if [ "$first" -lt 1 ] || [ "$last" -lt "$first" ] || [ "$last" -gt "$pages" ]; then
      echo "invalid PDF_VISUAL_PAGE_FROM/PDF_VISUAL_PAGE_TO for $pdf: $first-$last of $pages" >&2
      exit 1
    fi
    selected="$dir/input-p$first-$last.pdf"
    mkdir -p "$dir/pages"
    pdfseparate -f "$first" -l "$last" "$pdf" "$dir/pages/page-%03d.pdf" >"$dir/pdfseparate.log" 2>&1
    # shellcheck disable=SC2046
    pdfunite $(find "$dir/pages" -name 'page-*.pdf' | sort) "$selected" >"$dir/pdfunite.log" 2>&1
  elif [ "$max_pages" -gt 0 ] && [ "$pages" -gt "$max_pages" ]; then
    selected="$dir/input-p1-$max_pages.pdf"
    mkdir -p "$dir/pages"
    pdfseparate -f 1 -l "$max_pages" "$pdf" "$dir/pages/page-%03d.pdf" >"$dir/pdfseparate.log" 2>&1
    # shellcheck disable=SC2046
    pdfunite $(find "$dir/pages" -name 'page-*.pdf' | sort) "$selected" >"$dir/pdfunite.log" 2>&1
  fi

  printf '%s' "$selected"
}

render_pair() {
  local input_pdf="$1"
  local translated_pdf="$2"
  local dir="$3"

  mkdir -p "$dir/rendered/orig" "$dir/rendered/trans" "$dir/rendered/side-by-side"
  mutool draw -r "$dpi" -o "$dir/rendered/orig/page-%03d.png" "$input_pdf" >"$dir/render-orig.log" 2>&1
  mutool draw -r "$dpi" -o "$dir/rendered/trans/page-%03d.png" "$translated_pdf" >"$dir/render-trans.log" 2>&1

  for orig in "$dir"/rendered/orig/page-*.png; do
    [ -e "$orig" ] || continue
    local leaf
    leaf="$(basename "$orig")"
    convert "$orig" "$dir/rendered/trans/$leaf" +append "$dir/rendered/side-by-side/$leaf"
  done
}

for pdf in "$@"; do
  if [ ! -f "$pdf" ]; then
    echo "not a file: $pdf" >&2
    continue
  fi

  pages="$(page_count "$pdf")"
  if [ -z "$pages" ]; then
    echo "could not read page count: $pdf" >&2
    continue
  fi

  name="$(sanitize_name "$pdf")"
  out_dir="$out_root/$name"
  work_dir="$out_dir/work"
  rm -rf "$out_dir"
  mkdir -p "$work_dir"

  input_pdf="$(prepare_input_pdf "$pdf" "$work_dir" "$pages")"
  echo "[$name] identity smoke: pages=$pages input=$input_pdf"

  PDF_TEST_FILE="$input_pdf" \
    PDF_SMOKE_BUCKET_DIR="$bucket" \
    PDF_SMOKE_TARGET_LANG="$target_lang" \
    PDF_SMOKE_FORCED_SOURCE_LANG="$source_lang" \
    PDF_SMOKE_DUMP_DIR="$out_dir" \
    cargo test --features pdf --test pdf_smoke -- --nocapture >"$out_dir/pdf-smoke.log" 2>&1

  render_pair "$input_pdf" "$out_dir/translated.pdf" "$out_dir"
  echo "[$name] wrote $out_dir"
done
