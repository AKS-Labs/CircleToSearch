#!/usr/bin/env python3

import argparse
import json
import os
from dataclasses import dataclass
from html import escape
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent


@dataclass
class VoiceSample:
    voice: str
    quality: str
    engine: str
    audio_href: str | None


@dataclass
class RegionGroup:
    label: str
    voices: list[VoiceSample]


@dataclass
class LanguageSection:
    name: str
    sample_text: str | None
    regions: list[RegionGroup]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate a static HTML page for TTS sample browsing.",
    )
    parser.add_argument(
        "index_json",
        nargs="?",
        default="/home/david/AndroidStudioProjects/bucket/index.json",
        help="Path to index.json",
    )
    parser.add_argument(
        "--samples-root",
        default="samples_ogg",
        help="Root directory containing generated .opus samples. Default: samples_ogg",
    )
    parser.add_argument(
        "--output",
        default="samples.html",
        help="Output HTML path. Default: samples.html",
    )
    parser.add_argument(
        "--sample-texts",
        default=str(REPO_ROOT / "data" / "sample_texts.json"),
        help="Path to sample_texts.json. Default: data/sample_texts.json in the repo",
    )
    return parser.parse_args()


def region_flag(region: str) -> str:
    if len(region) != 2 or not region.isalpha():
        return ""
    return "".join(chr(0x1F1E6 + ord(ch.upper()) - ord("A")) for ch in region)


def normalize_sample_name(value: str) -> str:
    chars: list[str] = []
    last_was_sep = False
    for ch in value:
        keep = ch.isalnum() or ch in "_.-"
        if keep:
            chars.append(ch)
            last_was_sep = False
        elif not last_was_sep:
            chars.append("_")
            last_was_sep = True
    return "".join(chars).strip("_")


def legacy_normalize_sample_name(value: str) -> str:
    return "".join(ch if ch.isascii() and (ch.isalnum() or ch in "_.-") else "_" for ch in value)


def resolve_audio(samples_root: Path, language: str, voice: str, quality: str | None) -> Path | None:
    quality = quality or "default"
    candidates = [
        samples_root / language / f"{normalize_sample_name(voice)}_{normalize_sample_name(quality)}.opus",
        samples_root / language / f"{legacy_normalize_sample_name(voice)}_{legacy_normalize_sample_name(quality)}.opus",
    ]
    return next((p for p in candidates if p.exists()), None)


def build_voice(pack: dict, samples_root: Path, language_code: str, output_dir: Path) -> VoiceSample:
    voice = pack.get("voice", "")
    quality = pack.get("quality", "")
    audio_path = resolve_audio(samples_root, language_code, voice, quality)
    href = os.path.relpath(audio_path, output_dir) if audio_path else None
    return VoiceSample(
        voice=voice,
        quality=quality,
        engine=pack.get("engine", ""),
        audio_href=href,
    )


def build_region(region_code: str, region_info: dict, packs: dict, samples_root: Path, language_code: str, output_dir: Path) -> RegionGroup | None:
    display_name = region_info.get("displayName") or region_code
    flag = region_flag(region_code)
    label = f"{flag} {display_name}".strip()
    voices = [
        build_voice(packs[pack_id], samples_root, language_code, output_dir)
        for pack_id in region_info.get("voices", [])
        if pack_id in packs and packs[pack_id].get("feature") == "tts"
    ]
    if not voices:
        return None
    return RegionGroup(label=label, voices=voices)


def build_language(language_code: str, language: dict, packs: dict, sample_texts: dict, samples_root: Path, output_dir: Path) -> LanguageSection | None:
    tts = language.get("tts")
    if not tts:
        return None
    meta = language.get("meta", {})
    name = meta.get("name") or meta.get("shortName") or language_code
    regions = [
        region
        for region_code, region_info in sorted(tts.get("regions", {}).items())
        if (region := build_region(region_code, region_info, packs, samples_root, language_code, output_dir))
    ]
    if not regions:
        return None
    return LanguageSection(
        name=name,
        sample_text=sample_texts.get(language_code),
        regions=regions,
    )


def render_voice(v: VoiceSample) -> str:
    if v.audio_href:
        sample = f'<audio controls preload="none" src="{escape(v.audio_href)}"></audio>'
    else:
        sample = '<span class="missing">missing</span>'
    return (
        "<tr>"
        f'<td data-label="Voice">{escape(v.voice)}</td>'
        f'<td data-label="Quality">{escape(v.quality)}</td>'
        f'<td data-label="Engine">{escape(v.engine)}</td>'
        f'<td data-label="Sample" class="cell-player">{sample}</td>'
        "</tr>"
    )


def render_region(r: RegionGroup) -> str:
    rows = "\n".join(render_voice(v) for v in r.voices)
    return f"""<div class="region-block">
  <h3>{escape(r.label)}</h3>
  <table>
    <colgroup>
      <col class="col-voice"><col class="col-quality"><col class="col-engine"><col class="col-player">
    </colgroup>
    <thead><tr><th>Voice</th><th>Quality</th><th>Engine</th><th>Sample</th></tr></thead>
    <tbody>
{rows}
    </tbody>
  </table>
</div>"""


def render_language(lang: LanguageSection) -> str:
    sample = f'<p class="sample-text">{escape(lang.sample_text)}</p>\n' if lang.sample_text else ""
    regions = "\n".join(render_region(r) for r in lang.regions)
    anchor = escape(lang.name.replace(" ", "-"), quote=True)
    return f"""<section>
<h2 id="{anchor}"><a href="#{anchor}">{escape(lang.name)}</a></h2>
{sample}{regions}
</section>"""


def render_document(languages: list[LanguageSection], css: str) -> str:
    body = "\n".join(render_language(l) for l in languages) if languages else "<p>No TTS rows found.</p>"
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>TTS Samples</title>
<style>{css}</style>
</head>
<body>
<main>
<h1>TTS Samples</h1>
{body}
</main>
</body>
</html>
"""


def main() -> int:
    args = parse_args()
    index_path = Path(args.index_json).resolve()
    samples_root = Path(args.samples_root).resolve()
    output_path = Path(args.output).resolve()
    sample_texts_path = Path(args.sample_texts).resolve()

    data = json.loads(index_path.read_text(encoding="utf-8"))
    sample_texts = (
        json.loads(sample_texts_path.read_text(encoding="utf-8"))
        if sample_texts_path.exists()
        else {}
    )
    packs = data.get("packs", {})
    languages_raw = data.get("languages", {})

    languages = [
        section
        for code, lang in languages_raw.items()
        if (section := build_language(code, lang, packs, sample_texts, samples_root, output_path.parent))
    ]
    languages.sort(key=lambda s: s.name.casefold())

    css = (SCRIPT_DIR / "samples.css").read_text(encoding="utf-8")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(render_document(languages, css), encoding="utf-8")
    print(f"Wrote {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
