"""The pure half of the eval-leak exclusion: spec parsing, source reading,
index building and row filtering, none of which touch the filesystem."""

from __future__ import annotations

import json
import pathlib

import pytest

from exclude_eval import (
    EvalPair,
    EvalSource,
    ExclusionIndex,
    SourceKind,
    build_index,
    filter_rows,
    parse_pair,
    parse_source,
    source_lines,
    zip_pair,
)
from pair_spec import sha


def index_of(*pairs: tuple[EvalSource, list[str]]) -> ExclusionIndex:
    return build_index(list(pairs))


def index_of_pairs(*loaded: tuple[EvalPair, list[tuple[str, str]]]) -> ExclusionIndex:
    return build_index([], list(loaded))


TEXT = EvalSource(pathlib.Path("probes/check.en"), SourceKind.TEXT, ())
OTHER = EvalSource(pathlib.Path("probes/adversarial.en"), SourceKind.TEXT, ())
ONEWORD = EvalPair(
    EvalSource(pathlib.Path("eval_ka2en/oneword_ho.src"), SourceKind.TEXT, ()),
    EvalSource(pathlib.Path("eval_ka2en/oneword_ho.ref"), SourceKind.TEXT, ()),
)


def test_parse_plain_text_source():
    assert parse_source("probes/check.en") == TEXT


def test_parse_digest_list():
    s = parse_source("data/eval_exclude.sha256")
    assert s.kind is SourceKind.DIGESTS and s.fields == ()


def test_parse_jsonl_requires_fields():
    # Guessing which fields carry eval text is exactly how a set gets
    # half-excluded, so an unqualified .jsonl is refused rather than defaulted.
    with pytest.raises(ValueError, match="must name its fields"):
        parse_source("probes/check.ka.gen.jsonl")


def test_parse_jsonl_with_fields():
    s = parse_source("probes/check.ka.gen.jsonl:en,ka")
    assert s.kind is SourceKind.JSONL and s.fields == ("en", "ka")


def test_fields_are_refused_on_a_text_source():
    with pytest.raises(ValueError, match="only .jsonl sources take fields"):
        parse_source("probes/check.en:en")


def test_text_source_holds_out_every_column():
    lines = source_lines(TEXT, "Fire Exit\tგასასვლელი\n\nPush\n")
    assert lines == ["Fire Exit", "გასასვლელი", "Push"]


def test_digest_source_reads_bare_digests():
    body = f"{sha('Push')}\n{sha('Pull')}\n"
    assert source_lines(parse_source("x.sha256"), body) == sorted(
        {sha("Push"), sha("Pull")})


def test_jsonl_source_reads_only_the_named_fields():
    body = json.dumps({"en": "Push", "ka": "დააჭირეთ", "category": "signs"}) + "\n"
    src = parse_source("x.jsonl:en,ka")
    assert source_lines(src, body) == ["Push", "დააჭირეთ"]


def test_jsonl_null_field_is_skipped_not_held_out():
    # `verified: null` is unset, not an eval string; holding out the empty
    # string would match every blank column in the corpus.
    body = json.dumps({"en": "Push", "ka_alt": None}) + "\n"
    assert source_lines(parse_source("x.jsonl:en,ka_alt"), body) == ["Push"]


def test_jsonl_missing_field_is_an_error():
    with pytest.raises(ValueError, match="missing field"):
        source_lines(parse_source("x.jsonl:en,ka"),
                     json.dumps({"en": "Push"}) + "\n")


def test_exact_line_is_dropped():
    idx = index_of((TEXT, ["Fire Extinguisher"]))
    kept, drops = filter_rows(["Fire Extinguisher\tცეცხლმაქრი"], idx, 2)
    assert kept == [] and len(drops) == 1
    assert drops[0].matched == "probes/check.en"


def test_recased_and_repunctuated_line_is_dropped():
    """The digest misses these; the normalised key is why both exist."""
    idx = index_of((TEXT, ["Do not immerse the device in water."]))
    kept, _ = filter_rows(["do not immerse the device in water\tX"], idx, 2)
    assert kept == []


def test_target_column_leak_is_dropped():
    idx = index_of((TEXT, ["ცეცხლმაქრი"]))
    kept, _ = filter_rows(["Fire Extinguisher\tცეცხლმაქრი"], idx, 2)
    assert kept == []


def test_alignment_column_is_not_compared():
    """A 3-col guided-alignment TSV's third field is a Pharaoh string. Text
    columns are bounded so it can never be read as corpus text."""
    idx = index_of((TEXT, ["0-0 1-1"]))
    rows = ["Push\tდააჭირეთ\t0-0 1-1"]
    assert filter_rows(rows, idx, 2)[0] == rows
    assert filter_rows(rows, idx, 3)[0] == []


def test_clean_rows_survive_and_keep_their_order():
    idx = index_of((TEXT, ["Push"]))
    rows = ["Pull\tმოქაჩეთ", "Push\tდააჭირეთ", "Exit\tგასასვლელი"]
    kept, drops = filter_rows(rows, idx, 2)
    assert kept == ["Pull\tმოქაჩეთ", "Exit\tგასასვლელი"]
    assert drops[0].line_number == 2


def test_digest_only_source_matches_the_raw_line():
    src = parse_source("data/eval_exclude.sha256")
    idx = index_of((src, [sha("Keep out of reach of children")]))
    kept, _ = filter_rows(["Keep out of reach of children\tX"], idx, 2)
    assert kept == []


def test_a_digest_does_not_match_a_recased_line():
    """Digest lists carry no text to normalise, so they are exact by
    construction; the report must not imply otherwise."""
    src = parse_source("data/eval_exclude.sha256")
    idx = index_of((src, [sha("Push")]))
    assert filter_rows(["push\tდააჭირეთ"], idx, 2)[0] != []


def test_attribution_is_first_match_so_counts_do_not_double():
    idx = index_of((TEXT, ["Push"]), (OTHER, ["Push"]))
    _, drops = filter_rows(["Push\tდააჭირეთ"], idx, 2)
    assert [d.matched for d in drops] == ["probes/check.en"]


def test_blank_columns_never_match():
    idx = index_of((TEXT, ["Push"]))
    rows = ["\t", "   \tდააჭირეთ"]
    assert filter_rows(rows, idx, 2)[0] == rows


def test_pair_parses_two_specs():
    pair = parse_pair(["a.src", "b.jsonl:ka"])
    assert pair.left.kind is SourceKind.TEXT
    assert pair.right.kind is SourceKind.JSONL and pair.right.fields == ("ka",)
    assert pair.tag == "a.src+b.jsonl"


def test_pair_refuses_a_digest_list():
    with pytest.raises(ValueError, match="cannot be one side"):
        parse_pair(["data/eval_exclude.sha256", "x.ref"])


def test_pair_refuses_misaligned_sides():
    """Zipping to the shorter side would hold out mis-shifted pairs, which is
    worse than holding out nothing."""
    with pytest.raises(ValueError, match="must be aligned"):
        zip_pair(ONEWORD, ["ა", "ბ"], ["A"])


def test_pair_drops_the_row_that_carries_both_sides():
    idx = index_of_pairs((ONEWORD, [("დუღილი", "Boil")]))
    kept, drops = filter_rows(["დუღილი\tBoil"], idx, 2)
    assert kept == [] and drops[0].matched == ONEWORD.tag


def test_pair_keeps_a_row_that_shares_only_the_reference():
    """The 435-row defect: "Boil" is also the reference of a different Georgian
    word, and that row leaks nothing."""
    idx = index_of_pairs((ONEWORD, [("დუღილი", "Boil")]))
    rows = ["ადუღება\tBoil"]
    assert filter_rows(rows, idx, 2)[0] == rows


def test_pair_keeps_a_row_that_shares_only_the_source():
    """Held-out sources are still passed plainly with --eval; the PAIR rule on
    its own must not drop a row that merely reuses the source string."""
    idx = index_of_pairs((ONEWORD, [("დუღილი", "Boil")]))
    rows = ["დუღილი\tBoiling"]
    assert filter_rows(rows, idx, 2)[0] == rows


def test_pair_matches_in_either_column_order():
    """en->ka writes (en, ka) and ka->en writes (ka, en); the same held-out
    pair has to be found in both."""
    idx = index_of_pairs((ONEWORD, [("დუღილი", "Boil")]))
    assert filter_rows(["Boil\tდუღილი"], idx, 2)[0] == []


def test_pair_is_normalised_like_a_plain_source():
    idx = index_of_pairs((ONEWORD, [("დუღილი.", "Boil")]))
    assert filter_rows(["დუღილი\tboil"], idx, 2)[0] == []


def test_pair_ignores_the_alignment_column():
    idx = index_of_pairs((ONEWORD, [("დუღილი", "0-0")]))
    rows = ["დუღილი\tBoil\t0-0"]
    assert filter_rows(rows, idx, 2)[0] == rows


def test_pair_with_a_blank_side_is_not_held_out():
    """A pair whose reference normalises away would otherwise match every row
    that carries the source alongside a punctuation-only column."""
    idx = index_of_pairs((ONEWORD, [("დუღილი", "--")]))
    rows = ["დუღილი\t--"]
    assert filter_rows(rows, idx, 2)[0] == rows


def test_plain_and_pair_sources_coexist():
    idx = build_index([(TEXT, ["Push"])], [(ONEWORD, [("დუღილი", "Boil")])])
    kept, drops = filter_rows(
        ["Push\tდააჭირეთ", "დუღილი\tBoil", "ადუღება\tBoil"], idx, 2)
    assert kept == ["ადუღება\tBoil"]
    assert [d.matched for d in drops] == ["probes/check.en", ONEWORD.tag]
