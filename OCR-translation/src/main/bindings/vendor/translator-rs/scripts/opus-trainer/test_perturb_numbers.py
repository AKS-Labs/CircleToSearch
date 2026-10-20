"""The pure half of the number perturbation: eligibility, the shape it prints a
replacement in, and the invariant the whole thing exists for, which is that the
two sides of every emitted row carry the same figures."""

from __future__ import annotations

import random

import pytest

from number_fidelity import Verdict, compare_line
from number_fidelity import parse_config as parse_number_config
from perturb_numbers import (
    Dose,
    Emit,
    Role,
    Row,
    Scalar,
    Skip,
    draw_integer,
    group_thousands,
    number_words_present,
    parse_config,
    parse_dose,
    parse_row,
    perturb_row,
    scan,
)

CONFIG = parse_config({
    "number_words": {"en": {"twice": "2", "one": "1"}, "ka": {"ორჯერ": "2"}},
    "numeral_word_suffixes": {"ka": ["ჯერ", "ჯერად"]},
})

FIDELITY = parse_number_config({
    "currencies": {"GEL": ["₾", "gel", "lari"], "USD": ["$", "usd"]},
    "number_words": {"en": {"twice": "2"}, "ka": {"ორჯერ": "2"}},
    "currency_symbols": {"GEL": "₾"},
    "currency_words": {"en": {"GEL": "lari"}},
})


def run(source: str, target: str, trailing: tuple[str, ...] = (), variants: int = 3, seed: str = "t"):
    return perturb_row(Row(source, target, trailing), CONFIG, "ka", "en",
                       Dose(Emit.APPEND, variants, 1.0), random.Random(seed))


def replace(source: str, target: str, share: float = 1.0, seed: str = "t"):
    return perturb_row(Row(source, target, ()), CONFIG, "ka", "en",
                       Dose(Emit.REPLACE, 1, share), random.Random(seed))


def variants_of(result):
    return [line.split("\t")[:2] for line in result.lines[1:]]


def test_a_row_with_no_figures_is_emitted_once():
    result = run("გამარჯობა", "Hello")
    assert result.skip is Skip.NO_FIGURES
    assert result.lines == ("გამარჯობა\tHello",)


def test_sides_that_disagree_on_their_figures_are_never_repaired():
    result = run("ოთახი 250", "Room 350")
    assert result.skip is Skip.FIGURES_DISAGREE
    assert result.lines == ("ოთახი 250\tRoom 350",)


def test_a_spelled_out_figure_makes_the_row_ineligible():
    """"twice" would still say two after the digit moved, so the row stays put."""
    assert run("მიიღეთ 2-ჯერ დღეში", "Take 2 times daily").skip is Skip.NUMBER_WORDS
    assert run("ოთახი 2", "Room 2, one of two").skip is Skip.NUMBER_WORDS


def test_a_numeral_suffix_counts_as_a_number_word():
    assert number_words_present("2-ჯერ დღეში", CONFIG.words_of("ka"), CONFIG.suffixes_of("ka"))
    assert not number_words_present("25 Nm-მდე", CONFIG.words_of("ka"), CONFIG.suffixes_of("ka"))


def test_an_eligible_row_keeps_the_original_and_adds_the_variants():
    result = run("ოთახი 404", "Room 404")
    assert result.skip is None
    assert result.lines[0] == "ოთახი 404\tRoom 404"
    assert len(result.lines) == 4


def test_both_sides_get_the_same_replacement():
    for source, target in variants_of(run("ოთახი 404, სართული 3", "Room 404, floor 3")):
        assert compare_line(0, source, target, FIDELITY, "ka", "en").verdict is Verdict.OK
        assert source != "ოთახი 404, სართული 3"


def test_the_replacement_is_shared_across_two_spellings_of_one_value():
    """"8,75" and "8.75" are one value, so one draw has to satisfy both shapes."""
    for source, target in variants_of(run("ფასი 8,75 ₾", "Price 8.75 GEL")):
        assert compare_line(0, source, target, FIDELITY, "ka", "en").verdict is Verdict.OK
        assert "," in source and "." in target


def test_thousands_grouping_survives_a_longer_replacement():
    for source, target in variants_of(run("1 250 ცალი", "1,250 pieces", variants=8)):
        assert compare_line(0, source, target, FIDELITY, "ka", "en").verdict is Verdict.OK
        assert len(source.split()) == 3 and len(target.split()) == 2


def test_leading_zeros_are_part_of_the_shape():
    for source, target in variants_of(run("შეცდომა 07", "Error 07", variants=8)):
        assert source.split()[1] == target.split()[1]
        assert source.split()[1].startswith("0") and len(source.split()[1]) == 2


def test_a_time_stays_a_time():
    for source, target in variants_of(run("მატარებელი 14:05", "Train at 14:05", variants=10)):
        hour, minute = target.split()[-1].split(":")
        assert 0 <= int(hour) <= 23 and 0 <= int(minute) <= 59
        assert len(minute) == 2


def test_a_dotted_date_stays_a_date():
    for source, target in variants_of(run("თარიღი 12.05.2024", "Date 12.05.2024", variants=10)):
        day, month, year = target.split()[-1].split(".")
        assert 1 <= int(day) <= 28 and 1 <= int(month) <= 12 and 1900 <= int(year) <= 2099


def test_a_slashed_date_keeps_its_components_valid():
    for source, target in variants_of(run("12/05/2024 წელი", "12/05/2024", variants=10)):
        day, month, year = target.split("/")
        assert 1 <= int(day) <= 28 and 1 <= int(month) <= 12 and 1900 <= int(year) <= 2099


def test_a_colon_that_is_not_a_clock_leaves_both_figures_free():
    """ISO 10012:2003 is the ft6 failure; neither half is an hour or a minute."""
    occurrences = scan("ISO 10012:2003", 0)
    assert [o.role for o in occurrences] == [Role.PLAIN, Role.PLAIN]
    seen = set()
    for source, target in variants_of(run("ISO 10012:2003", "ISO 10012:2003", variants=10)):
        assert source == target
        seen.add(source)
    assert len(seen) > 1


def test_a_suffix_welded_to_the_figure_is_left_alone():
    for source, target in variants_of(run("25 Nm-მდე", "up to 25 Nm", variants=5)):
        assert source.endswith(" Nm-მდე")
        assert compare_line(0, source, target, FIDELITY, "ka", "en").verdict is Verdict.OK


def test_the_alignment_column_travels_with_every_variant():
    result = run("ოთახი 404", "Room 404", trailing=("0-0 1-1",))
    assert all(line.endswith("\t0-0 1-1") for line in result.lines)
    assert all(len(line.split("\t")) == 3 for line in result.lines)


def test_token_counts_never_move():
    result = run("ფასი 1 250,75 ₾ და 07", "Price 1,250.75 GEL and 07", variants=12)
    for source, target in variants_of(result):
        assert len(source.split()) == 6
        assert len(target.split()) == 5


def test_the_same_seed_gives_the_same_rows():
    assert run("ოთახი 404", "Room 404", seed="a").lines == run("ოთახი 404", "Room 404", seed="a").lines
    assert run("ოთახი 404", "Room 404", seed="a").lines != run("ოთახი 404", "Room 404", seed="b").lines


def test_lengths_are_long_tailed_not_the_original_length():
    """The point of the corpus change: four- to seven-digit runs become ordinary."""
    lengths = []
    for row in range(400):
        for source, _ in variants_of(run("ოთახი 404", "Room 404", seed=f"s{row}", variants=1)):
            lengths.append(len(source.split()[1]))
    assert sum(1 for n in lengths if n == 3) / len(lengths) < 0.55
    assert sum(1 for n in lengths if 4 <= n <= 7) / len(lengths) > 0.35


@pytest.mark.parametrize(("token", "shape"), [
    ("404", Scalar(3, 0, "", "", 0)),
    ("07", Scalar(2, 1, "", "", 0)),
    ("1,250", Scalar(4, 0, ",", "", 0)),
    ("1 250", Scalar(4, 0, " ", "", 0)),
    ("8,75", Scalar(1, 0, "", ",", 2)),
    ("1,250.75", Scalar(4, 0, ",", ".", 2)),
])
def test_shapes_are_read_the_way_the_fidelity_measure_reads_them(token, shape):
    assert scan(token, 0)[0].scalar == shape


@pytest.mark.parametrize(("digits", "separator", "grouped"), [
    ("1234567", ",", "1,234,567"),
    ("123", ",", "123"),
    ("1234", " ", "1 234"),
    ("1234567", "", "1234567"),
])
def test_grouping_puts_the_separator_every_three_digits(digits, separator, grouped):
    assert group_thousands(digits, separator) == grouped


def test_an_all_zero_figure_has_nowhere_to_go():
    assert draw_integer(random.Random("z"), Scalar(3, 3, "", "", 0), False) == "000"


def test_a_row_needs_two_columns():
    with pytest.raises(ValueError, match="source and a target"):
        parse_row("only one column", 7)


def test_replace_emits_one_row_for_every_input_row():
    """The dose ft8 needs: the corpus keeps its size, so it keeps its register mix."""
    for source, target in (("ოთახი 404", "Room 404"), ("გამარჯობა", "Hello"),
                           ("ოთახი 250", "Room 350"), ("მიიღეთ 2-ჯერ", "Take 2 times")):
        assert len(replace(source, target).lines) == 1


def test_a_replaced_row_carries_the_new_figure_and_not_the_old_one():
    result = replace("ოთახი 404, სართული 3", "Room 404, floor 3")
    assert result.perturbed == 1
    source, target = result.lines[0].split("\t")
    assert source != "ოთახი 404, სართული 3"
    assert compare_line(0, source, target, FIDELITY, "ka", "en").verdict is Verdict.OK


def test_a_share_below_one_leaves_the_rest_of_the_eligible_rows_as_they_were():
    kept = [replace("ოთახი 404", "Room 404", share=0.7, seed=f"s{i}") for i in range(400)]
    untouched = [r for r in kept if r.perturbed == 0]
    assert all(r.lines == ("ოთახი 404\tRoom 404",) for r in untouched)
    assert all(r.skip is None for r in kept)
    assert 0.6 < 1 - len(untouched) / len(kept) < 0.8


def test_a_share_of_zero_is_not_a_dose():
    with pytest.raises(ValueError, match="fraction of the eligible rows"):
        parse_dose(Emit.REPLACE, None, 0.0)


def test_the_two_flags_belong_to_the_mode_that_uses_them():
    with pytest.raises(ValueError, match="--share applies to replace mode"):
        parse_dose(Emit.APPEND, 3, 0.7)
    with pytest.raises(ValueError, match="--variants applies to append mode"):
        parse_dose(Emit.REPLACE, 3, 0.7)
    assert parse_dose(Emit.APPEND, 3, 1.0) == Dose(Emit.APPEND, 3, 1.0)
    assert parse_dose(Emit.APPEND, None, 1.0) == Dose(Emit.APPEND, 3, 1.0)
    assert parse_dose(Emit.REPLACE, None, 0.7) == Dose(Emit.REPLACE, 1, 0.7)


def test_append_at_full_share_is_the_run_it_always_was():
    """ft7's corpus has to come back from ft7's seed, so APPEND draws no share."""
    assert run("ოთახი 404", "Room 404", seed="ka-ft7").lines == perturb_row(
        Row("ოთახი 404", "Room 404", ()), CONFIG, "ka", "en",
        Dose(Emit.APPEND, 3, 1.0), random.Random("ka-ft7")).lines
    assert run("ოთახი 404", "Room 404", seed="ka-ft7").perturbed == 3
