"""The pure half of the tranche rewrite: the figure test, the two mirrors, the
lowercase vocabulary and the casing rule. No filesystem, no generator calls."""

from __future__ import annotations

import pytest

from build_tranche import (
    TranchePair,
    figures_agree,
    is_title_case,
    lowercase_vocabulary,
    mirror_currency,
    mirror_numerals,
    rewrite_rows,
    sentence_case,
)
from number_fidelity import parse_config

CONFIG = parse_config({
    "currencies": {"GEL": ["₾", "gel", "lari", "ლარი"], "USD": ["$", "usd", "dollar", "dollars"]},
    "number_words": {"en": {"one": "1", "six": "6", "twice": "2"}},
    "currency_symbols": {"GEL": "₾", "USD": "$"},
    "currency_words": {"en": {"GEL": "lari"}, "ka": {"GEL": "ლარი"}},
})


def agree(source: str, target: str) -> bool:
    return figures_agree(source, target, CONFIG, "ka", "en")


def test_a_row_whose_figures_differ_does_not_agree():
    assert not agree("2387 შემთხვევა", "237 cases")


def test_a_convention_difference_still_agrees():
    assert agree("1 500 ვტ", "1,500 W")


def test_a_spelled_out_small_integer_still_agrees():
    assert agree("6 თვე", "six months")


def test_a_currency_difference_does_not_decide_agreement():
    """Currency is what the rewrite fixes, so it must not send the row to the bin."""
    assert agree("86,40 ₾", "86.40 GEL")


def test_numerals_take_the_source_form():
    assert mirror_numerals("ნომინალური სიმძლავრე 1 500 ვტ", "Nominal power 1,500 W") \
        == "Nominal power 1 500 W"
    assert mirror_numerals("ლაქტოზა 0,1%", "Lactose 0.1%") == "Lactose 0,1%"


def test_a_repeated_value_keeps_each_of_its_source_forms():
    assert mirror_numerals("1 000 და 1000", "1,000 and 1,000") == "1 000 and 1000"


def test_a_symbol_source_gives_the_target_the_symbol():
    assert mirror_currency("სულ 86,40 ₾", "Total 86.40 GEL", CONFIG, "en") == "Total 86.40 ₾"


def test_a_word_source_gives_the_target_its_own_language_word():
    assert mirror_currency("სულ 21,45 ლარი", "Total 21.45 GEL", CONFIG, "en") == "Total 21.45 lari"


def test_a_missing_marker_is_not_invented():
    assert mirror_currency("სულ 86,40 ₾", "Total 86.40", CONFIG, "en") == "Total 86.40"


def test_two_currencies_are_left_alone():
    """Which marker belongs to which figure is a parse this rewrite does not do."""
    text = "Total 10 GEL or 4 USD"
    assert mirror_currency("სულ 10 ₾ ან 4 $", text, CONFIG, "en") == text


def test_title_case_needs_more_than_one_word():
    assert not is_title_case("Settings")
    assert is_title_case("Picture Mode")
    assert is_title_case("Read Directory A")
    assert not is_title_case("Picture mode")


def test_lowercase_vocabulary_is_what_the_corpus_writes_down():
    corpus = ["Set the mode now", "change the mode now", "the mode is set now",
              "Open the mode now", "Contact KAB now"]
    common = lowercase_vocabulary(corpus)
    assert "mode" in common
    assert "the" in common
    assert "kab" not in common


def test_sentence_case_lowers_only_common_words():
    common = frozenset({"mode", "settings", "media"})
    assert sentence_case("Picture Mode", common) == "Picture mode"
    assert sentence_case("USB Media", common) == "USB media"
    assert sentence_case("Wi-Fi Settings", common) == "Wi-Fi settings"


def test_sentence_case_leaves_unit_symbols_and_dates_alone():
    common = frozenset({"a", "g", "may", "current", "signal"})
    assert sentence_case("Current 2.4 A", common) == "Current 2.4 A"
    assert sentence_case("Signal 4G", common) == "Signal 4G"
    assert sentence_case("Monday 12 May", common) == "Monday 12 May"


@pytest.mark.parametrize("register", ["ui", "signage"])
def test_rewrite_applies_casing_only_to_the_named_registers(register):
    rows = [TranchePair("სურათის რეჟიმი", "Picture Mode", register)] + [
        TranchePair("რეჟიმი", "the mode is set", "ui") for _ in range(20)
    ]
    out = rewrite_rows(rows, CONFIG, "ka", "en", frozenset({"ui"}))
    assert out[0].target == ("Picture mode" if register == "ui" else "Picture Mode")
    assert out[0].case_changed is (register == "ui")


def test_rewrite_drops_a_row_whose_figures_disagree():
    rows = [TranchePair("2387 შემთხვევა", "237 cases", "notices")]
    out = rewrite_rows(rows, CONFIG, "ka", "en", frozenset({"ui"}))
    assert out[0].dropped
    assert not out[0].numbers_changed


def test_rewrite_reports_a_number_change():
    rows = [TranchePair("სულ 86,40 ₾", "Total 86.40 GEL", "labels")]
    out = rewrite_rows(rows, CONFIG, "ka", "en", frozenset({"ui"}))
    assert out[0].target == "Total 86,40 ₾"
    assert out[0].numbers_changed
