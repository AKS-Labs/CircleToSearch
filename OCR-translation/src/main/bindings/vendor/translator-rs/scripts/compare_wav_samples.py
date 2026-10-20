#!/usr/bin/env python3

import argparse
import math
import statistics
import sys
import wave
from array import array
from dataclasses import dataclass
from pathlib import Path


@dataclass
class PauseSegment:
    start_sec: float
    end_sec: float

    @property
    def duration_sec(self) -> float:
        return self.end_sec - self.start_sec


@dataclass
class SampleMetrics:
    path: Path
    language: str
    duration_sec: float
    silence_sec: float
    speech_sec: float
    leading_silence_sec: float
    trailing_silence_sec: float
    internal_pause_sec: float
    internal_pause_count: int
    voiced_ratio: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare WAV TTS samples per language by duration and pause structure.",
    )
    parser.add_argument(
        "root",
        nargs="?",
        default="samples",
        help="Root samples directory. Default: samples",
    )
    parser.add_argument(
        "--lang",
        action="append",
        default=[],
        help="Limit to one or more language codes. Repeatable.",
    )
    parser.add_argument(
        "--threshold-db",
        type=float,
        default=-38.0,
        help="Window RMS below this dBFS counts as silence. Default: -38",
    )
    parser.add_argument(
        "--window-ms",
        type=float,
        default=20.0,
        help="Analysis window size in milliseconds. Default: 20",
    )
    parser.add_argument(
        "--min-pause-ms",
        type=float,
        default=140.0,
        help="Minimum contiguous silence to count as a pause. Default: 140",
    )
    parser.add_argument(
        "--duration-factor",
        type=float,
        default=1.35,
        help="Flag duration outliers beyond this ratio from the language median. Default: 1.35",
    )
    return parser.parse_args()


def read_samples(path: Path) -> tuple[list[int], int, int]:
    with wave.open(str(path), "rb") as wav:
        channels = wav.getnchannels()
        sample_width = wav.getsampwidth()
        frame_rate = wav.getframerate()
        frames = wav.readframes(wav.getnframes())

    if sample_width == 1:
        values = list(frames)
        values = [sample - 128 for sample in values]
    elif sample_width == 2:
        arr = array("h")
        arr.frombytes(frames)
        if sys.byteorder != "little":
            arr.byteswap()
        values = arr.tolist()
    elif sample_width == 4:
        arr = array("i")
        arr.frombytes(frames)
        if sys.byteorder != "little":
            arr.byteswap()
        values = arr.tolist()
    else:
        raise ValueError(f"Unsupported WAV sample width {sample_width} for {path}")

    return values, channels, frame_rate


def rms_dbfs(chunk: list[int], peak: float) -> float:
    if not chunk:
        return -120.0
    mean_square = sum(sample * sample for sample in chunk) / len(chunk)
    if mean_square <= 0.0:
        return -120.0
    rms = math.sqrt(mean_square)
    return 20.0 * math.log10(max(rms / peak, 1e-12))


def detect_silences(
    samples: list[int],
    channels: int,
    frame_rate: int,
    threshold_db: float,
    window_ms: float,
    min_pause_ms: float,
) -> tuple[list[PauseSegment], float]:
    if channels <= 0 or frame_rate <= 0:
        return [], 0.0

    total_frames = len(samples) // channels
    duration_sec = total_frames / frame_rate if frame_rate else 0.0
    if total_frames == 0:
        return [], duration_sec

    window_frames = max(1, int(round(frame_rate * window_ms / 1000.0)))
    min_pause_frames = max(1, int(round(frame_rate * min_pause_ms / 1000.0)))
    peak = float((1 << 15) - 1)
    if samples:
        max_abs = max(abs(sample) for sample in samples)
        if max_abs > peak:
            peak = float(max_abs)

    silent_ranges: list[PauseSegment] = []
    run_start_frame: int | None = None

    for start_frame in range(0, total_frames, window_frames):
        end_frame = min(total_frames, start_frame + window_frames)
        chunk = samples[start_frame * channels : end_frame * channels]
        silent = rms_dbfs(chunk, peak) < threshold_db
        if silent:
            if run_start_frame is None:
                run_start_frame = start_frame
        elif run_start_frame is not None:
            if start_frame - run_start_frame >= min_pause_frames:
                silent_ranges.append(
                    PauseSegment(
                        start_sec=run_start_frame / frame_rate,
                        end_sec=start_frame / frame_rate,
                    )
                )
            run_start_frame = None

    if run_start_frame is not None and total_frames - run_start_frame >= min_pause_frames:
        silent_ranges.append(
            PauseSegment(
                start_sec=run_start_frame / frame_rate,
                end_sec=total_frames / frame_rate,
            )
        )

    return silent_ranges, duration_sec


def analyze_file(
    path: Path,
    threshold_db: float,
    window_ms: float,
    min_pause_ms: float,
) -> SampleMetrics:
    samples, channels, frame_rate = read_samples(path)
    silences, duration_sec = detect_silences(
        samples,
        channels,
        frame_rate,
        threshold_db,
        window_ms,
        min_pause_ms,
    )

    leading = silences[0].duration_sec if silences and silences[0].start_sec <= 0.001 else 0.0
    trailing = silences[-1].duration_sec if silences and silences[-1].end_sec >= duration_sec - 0.001 else 0.0
    internal = [
        seg
        for seg in silences
        if seg.duration_sec > 0
        and seg.start_sec > 0.001
        and seg.end_sec < duration_sec - 0.001
    ]
    total_silence_sec = sum(seg.duration_sec for seg in silences)
    speech_sec = max(0.0, duration_sec - total_silence_sec)
    voiced_ratio = speech_sec / duration_sec if duration_sec else 0.0

    return SampleMetrics(
        path=path,
        language=path.parent.name,
        duration_sec=duration_sec,
        silence_sec=total_silence_sec,
        speech_sec=speech_sec,
        leading_silence_sec=leading,
        trailing_silence_sec=trailing,
        internal_pause_sec=sum(seg.duration_sec for seg in internal),
        internal_pause_count=len(internal),
        voiced_ratio=voiced_ratio,
    )


def median_metric(items: list[SampleMetrics], attr: str) -> float:
    return statistics.median(getattr(item, attr) for item in items)


def format_sec(value: float) -> str:
    return f"{value:5.2f}s"


def flags_for(metrics: SampleMetrics, group: list[SampleMetrics], duration_factor: float) -> list[str]:
    if len(group) < 2:
        return []

    flags: list[str] = []
    duration_med = median_metric(group, "duration_sec")
    internal_pause_med = median_metric(group, "internal_pause_sec")
    pause_count_med = median_metric(group, "internal_pause_count")
    lead_med = median_metric(group, "leading_silence_sec")
    trail_med = median_metric(group, "trailing_silence_sec")
    voiced_ratio_med = median_metric(group, "voiced_ratio")

    if duration_med > 0:
        ratio = metrics.duration_sec / duration_med
        if ratio > duration_factor:
            flags.append(f"long x{ratio:.2f}")
        elif ratio < 1.0 / duration_factor:
            flags.append(f"short x{1.0 / ratio:.2f}")

    if metrics.internal_pause_sec > max(internal_pause_med * 1.8, internal_pause_med + 0.35):
        flags.append(f"pause_time {metrics.internal_pause_sec:.2f}s")

    if metrics.internal_pause_count > max(pause_count_med * 1.8, pause_count_med + 2):
        flags.append(f"pause_count {metrics.internal_pause_count}")

    if metrics.leading_silence_sec > max(0.5, lead_med * 1.8, lead_med + 0.20):
        flags.append(f"lead {metrics.leading_silence_sec:.2f}s")

    if metrics.trailing_silence_sec > max(0.5, trail_med * 1.8, trail_med + 0.20):
        flags.append(f"trail {metrics.trailing_silence_sec:.2f}s")

    if metrics.voiced_ratio < min(voiced_ratio_med * 0.88, voiced_ratio_med - 0.08):
        flags.append(f"low_voice_ratio {metrics.voiced_ratio:.2f}")

    return flags


def print_language_report(language: str, items: list[SampleMetrics], duration_factor: float) -> None:
    rows: list[tuple[SampleMetrics, list[str]]] = []
    for metrics in sorted(items, key=lambda item: item.path.name):
        flags = flags_for(metrics, items, duration_factor)
        rows.append((metrics, flags))

    if len(items) < 2 or not any(flags for _, flags in rows):
        return

    print(f"\n== {language} ({len(items)} wavs) ==")
    print(
        "baseline "
        f"dur={format_sec(median_metric(items, 'duration_sec'))} "
        f" internal_pause={format_sec(median_metric(items, 'internal_pause_sec'))} "
        f" pause_count={median_metric(items, 'internal_pause_count'):.1f} "
        f" lead={format_sec(median_metric(items, 'leading_silence_sec'))} "
        f" trail={format_sec(median_metric(items, 'trailing_silence_sec'))} "
        f" voice_ratio={median_metric(items, 'voiced_ratio'):.2f}"
    )

    flagged_rows = [row for row in rows if row[1]]
    for metrics, flags in sorted(flagged_rows, key=lambda item: (-len(item[1]), item[0].duration_sec, item[0].path.name)):
        flag_text = " | flags: " + ", ".join(flags) if flags else ""
        print(
            f"{metrics.path.name:28} "
            f"dur={format_sec(metrics.duration_sec)} "
            f"speech={format_sec(metrics.speech_sec)} "
            f"pause={format_sec(metrics.internal_pause_sec)} "
            f"count={metrics.internal_pause_count:2d} "
            f"lead={format_sec(metrics.leading_silence_sec)} "
            f"trail={format_sec(metrics.trailing_silence_sec)} "
            f"voice={metrics.voiced_ratio:.2f}"
            f"{flag_text}"
        )


def main() -> int:
    args = parse_args()
    root = Path(args.root)
    if not root.exists():
        print(f"Missing samples root: {root}", file=sys.stderr)
        return 1

    wanted_langs = set(args.lang)
    per_language: dict[str, list[SampleMetrics]] = {}

    for path in sorted(root.glob("*/*.wav")):
        language = path.parent.name
        if wanted_langs and language not in wanted_langs:
            continue
        try:
            metrics = analyze_file(
                path,
                threshold_db=args.threshold_db,
                window_ms=args.window_ms,
                min_pause_ms=args.min_pause_ms,
            )
        except Exception as exc:
            print(f"skip {path}: {exc}", file=sys.stderr)
            continue
        per_language.setdefault(language, []).append(metrics)

    if not per_language:
        print("No WAV samples found.")
        return 1

    printed_any = False
    for language in sorted(per_language):
        before = printed_any
        print_language_report(language, per_language[language], args.duration_factor)
        if len(per_language[language]) >= 2 and any(
            flags_for(item, per_language[language], args.duration_factor)
            for item in per_language[language]
        ):
            printed_any = True

    if not printed_any:
        print("No flagged multi-voice language groups found.")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
