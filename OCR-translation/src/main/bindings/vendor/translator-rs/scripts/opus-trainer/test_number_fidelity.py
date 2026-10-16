"""The pure half of the number-fidelity measure: normalisation, the multiset
comparison and the slice roll-up, none of which touch the filesystem."""

from __future__ import annotations

import pytest

from number_fidelity import (
    Verdict,
    as_json,
    canonical_numeral,
    compare_line,
    currency_tokens,
    number_multiset,
    numeral_tokens,
    parse_config,
    score_slice,
    word_values,
)

CONFIG = parse_config({
    "currencies": {
        "GEL": ["₾", "gel", "lari", "ლარი"],
        "USD": ["$", "usd", "dollar", "dollars"],
        "EUR": ["€", "eur", "euro"],
        "GBP": ["£", "gbp", "pound", "pounds"],
    },
    "number_words": {
        "en": {"one": "1", "two": "2", "twice": "2", "half": "0.5"},
        "ka": {"ორჯერ": "2"},
    },
    "currency_symbols": {"GEL": "₾", "USD": "$"},
    "currency_words": {"en": {"GEL": "lari"}, "ka": {"GEL": "ლარი"}},
})


def compare(source: str, hypothesis: str) -> object:
    return compare_line(0, source, hypothesis, CONFIG, "ka", "en")


@pytest.mark.parametrize(("token", "canonical"), [
    ("404", "404"),
    ("07", "07"),
    ("1,250", "1250"),
    ("1.250", "1250"),
    ("1 250", "1250"),
    ("1'250", "1250"),
    ("12,50", "12.5"),
    ("12.50", "12.5"),
    ("12.5", "12.5"),
    ("1,250.75", "1250.75"),
    ("1 250,75", "1250.75"),
    ("0,01", "0.01"),
])
def test_separators_and_decimal_marks_collapse(token, canonical):
    assert canonical_numeral(token) == canonical


def test_leading_zeros_are_kept():
    """A panel that prints "07" and an overlay that prints "7" do not match."""
    assert compare("შეცდომა 07", "Error 7").verdict is Verdict.CORRUPTED


def test_colon_splits_a_time_into_two_numerals():
    assert numeral_tokens("14:05") == ["14", "05"]


def test_currency_spellings_fold_onto_one_token():
    assert currency_tokens("8,75 ₾", CONFIG) == ["cur:GEL"]
    assert currency_tokens("8.75 GEL", CONFIG) == ["cur:GEL"]
    assert currency_tokens("8.75 lari", CONFIG) == ["cur:GEL"]


def test_currency_word_inside_another_word_is_not_a_marker():
    assert currency_tokens("gelatin capsules", CONFIG) == []


def test_convention_difference_is_not_a_fidelity_defect():
    assert compare("1 კგ — 8,75 ₾", "1 kg - 8.75 GEL").verdict is Verdict.OK


def test_dropping_the_currency_is_a_defect():
    line = compare("ფასი 8,75 ₾", "Price 8.75")
    assert line.verdict is Verdict.OMITTED
    assert line.omitted == ("cur:GEL",)


def test_a_repeated_figure_is_an_addition_not_a_match():
    """The multiset is the point: a set would call the doubled time correct."""
    line = compare("მატარებელი 832, 14:05", "the 14:05 train at 14:05")
    assert line.verdict is Verdict.CORRUPTED
    assert line.omitted == ("832",)
    assert line.added == ("05", "14")


def test_number_word_rescues_an_omission():
    assert compare("მიიღეთ 2-ჯერ დღეში", "Take twice daily").verdict is Verdict.OK


def test_a_number_word_never_becomes_a_figure_of_its_own():
    """"one of the doors" is not a figure, so it neither adds nor satisfies one."""
    assert compare("კარი გაიღება", "one of the doors opens").verdict is Verdict.OK
    assert compare("ოთახი 12", "Room one").verdict is Verdict.OMITTED


def test_a_language_without_a_table_is_scored_on_digits_alone():
    line = compare_line(0, "მიიღეთ 2-ჯერ", "Take twice", CONFIG, "ka", "de")
    assert line.verdict is Verdict.OMITTED


def test_word_values_reads_only_the_named_language():
    assert word_values("Take twice", CONFIG.words("en")) == {"2": 1}
    assert word_values("Take twice", CONFIG.words("de")) == {}


def test_number_multiset_counts_numerals_and_currency_together():
    assert number_multiset("2 x 8,75 ₾", CONFIG) == {"2": 1, "8.75": 1, "cur:GEL": 1}


def test_lines_whose_source_has_no_figure_are_not_scored():
    scored = score_slice("s", ["გამარჯობა", "ოთახი 12"], ["Hello", "Room 12"], CONFIG, "ka", "en")
    assert scored.scored == 1
    assert scored.rate == 100.0


def test_slice_counts_split_the_four_verdicts():
    sources = ["ოთახი 12", "ოთახი 12", "ოთახი 12", "ოთახი 12"]
    hypotheses = ["Room 12", "Room", "Room 12 of 12", "Room 13"]
    scored = score_slice("s", sources, hypotheses, CONFIG, "ka", "en")
    assert dict(scored.counts) == {
        Verdict.OK: 1, Verdict.OMITTED: 1, Verdict.ADDED: 1, Verdict.CORRUPTED: 1,
    }
    assert scored.bad == 2
    assert [f.index for f in scored.failures] == [1, 2, 3]


def test_mismatched_line_counts_are_refused():
    with pytest.raises(ValueError, match="numbers"):
        score_slice("numbers", ["a"], ["a", "b"], CONFIG)


def test_json_carries_the_per_slice_and_total_counts():
    scored = score_slice("s", ["ოთახი 12"], ["Room"], CONFIG, "ka", "en")
    doc = as_json([scored])
    assert doc["slices"]["s"]["omitted"] == 1
    assert doc["total"]["bad"] == 1
    assert doc["slices"]["s"]["failures"][0]["omitted"] == ["12"]
