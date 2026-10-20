"""The pure half of the bilingual pair generator: spec, grid, parsing, gates.

Testable without a model call, which is the point of keeping pair_spec.py free of
I/O: every rule that decides whether a generated row enters the training set is
exercised here on rows written by hand, including the defects the shipped student
actually shows (a dropped figure, a Mtavruli heading, an English word left inside
the Georgian).
"""

from __future__ import annotations

import json
import pathlib

import pytest

from gen_pairs import load_exclusions, load_known
from pair_spec import (
    GateReason,
    NumberPolicy,
    PairRow,
    SpecError,
    build_jobs,
    fold,
    gate,
    load_spec,
    norm,
    numbers_agree,
    parse_rows,
    sha,
)

SPEC_PATH = pathlib.Path(__file__).parent / "configs" / "gen_pairs.ka.json"


@pytest.fixture(scope="module")
def spec():
    return load_spec(SPEC_PATH)


def pair(en: str, ka: str) -> PairRow:
    return PairRow(en=en, target=ka)


def check(spec, en: str, ka: str, band: str = "w04_08",
          numbers: NumberPolicy = NumberPolicy.FREE) -> GateReason | None:
    return gate(spec, pair(en, ka), spec.bands[band], numbers)


# ------------------------------------------------------------------ the spec


def test_ka_spec_loads_and_is_self_consistent(spec):
    assert spec.code == "ka"
    assert spec.sha and len(spec.sha) == 12
    assert {r.name for r in spec.registers} == {"signage", "labels", "menus", "ui", "notices"}
    numeric = spec.forms["numeric"]
    assert numeric.numbers is NumberPolicy.REQUIRED
    # Every register carries the numbers-bearing form, because number corruption
    # is a measured failure in every one of these settings, not just in dosages.
    assert all("numeric" in r.forms for r in spec.registers)


def test_form_naming_an_unknown_band_is_refused(tmp_path):
    raw = json.loads(SPEC_PATH.read_text(encoding="utf-8"))
    raw["forms"]["label"]["bands"] = ["w99"]
    p = tmp_path / "bad.json"
    p.write_text(json.dumps(raw), encoding="utf-8")
    with pytest.raises(SpecError, match="unknown bands"):
        load_spec(p)


def test_register_naming_an_unknown_form_is_refused(tmp_path):
    raw = json.loads(SPEC_PATH.read_text(encoding="utf-8"))
    raw["registers"][0]["forms"] = ["shouting"]
    p = tmp_path / "bad.json"
    p.write_text(json.dumps(raw), encoding="utf-8")
    with pytest.raises(SpecError, match="unknown forms"):
        load_spec(p)


def test_missing_field_is_one_error_not_a_crash_deeper_in(tmp_path):
    raw = json.loads(SPEC_PATH.read_text(encoding="utf-8"))
    del raw["gates"]["word_ratio"]
    p = tmp_path / "bad.json"
    p.write_text(json.dumps(raw), encoding="utf-8")
    with pytest.raises(SpecError, match="malformed spec"):
        load_spec(p)


# ------------------------------------------------------------------ the grid


def test_grid_covers_every_cell_once_per_round(spec):
    jobs = build_jobs(spec, rounds=1, have={})
    keys = [j.key for j in jobs]
    assert len(keys) == len(set(keys))
    cells = sum(len(r.categories) * sum(len(spec.forms[f].bands) for f in r.forms)
                for r in spec.registers)
    assert len(jobs) == cells
    assert len(build_jobs(spec, rounds=2, have={})) == 2 * cells


def test_grid_subsets_and_per_cell_override(spec):
    jobs = build_jobs(spec, rounds=1, have={}, registers=["menus"],
                      forms=["numeric"], bands=["w01_03"], per_cell=7)
    menus = next(r for r in spec.registers if r.name == "menus")
    assert len(jobs) == len(menus.categories)
    assert {j.n for j in jobs} == {7}
    assert {j.numbers for j in jobs} == {NumberPolicy.REQUIRED}


def test_unknown_register_is_refused_rather_than_silently_empty(spec):
    with pytest.raises(SpecError, match="unknown registers"):
        build_jobs(spec, rounds=1, have={}, registers=["signs"])


def test_prompt_carries_setting_form_band_and_the_have_list(spec):
    have = {"menu hot drinks": ["Espresso", "Green tea"], "": ["Exit"]}
    job = next(j for j in build_jobs(spec, 1, have, registers=["menus"],
                                     forms=["label"], bands=["w01_03"])
               if j.category == "menu hot drinks")
    assert "menu hot drinks" in job.prompt
    assert "one to three words" in job.prompt
    assert "ALREADY WRITTEN" in job.prompt
    assert "- Espresso" in job.prompt and "- Exit" in job.prompt
    assert '"ka"' in job.prompt and "Mkhedruli" in job.prompt


def test_prompt_omits_the_have_block_on_a_cold_start(spec):
    job = build_jobs(spec, 1, {}, registers=["menus"], forms=["label"], bands=["w01_03"])[0]
    assert "ALREADY WRITTEN" not in job.prompt


def test_numbers_rule_appears_only_in_numeric_cells(spec):
    numeric = build_jobs(spec, 1, {}, registers=["ui"], forms=["numeric"], bands=["w04_08"])[0]
    plain = build_jobs(spec, 1, {}, registers=["ui"], forms=["label"], bands=["w04_08"])[0]
    assert "EVERY row must carry at least one figure" in numeric.prompt
    assert "EVERY row must carry at least one figure" not in plain.prompt


# --------------------------------------------------------------- parsing rows


def test_a_bad_row_invalidates_only_itself(spec):
    payload = [
        {"en": "Exit", "ka": "გასასვლელი"},
        {"en": "Entrance"},
        "not an object",
        {"en": "", "ka": "შესასვლელი"},
        {"en": "Push", "ka": "  მიაწექით  "},
    ]
    rows, malformed = parse_rows(payload, spec)
    assert [r.en for r in rows] == ["Exit", "Push"]
    assert rows[1].target == "მიაწექით"
    assert malformed == 3


def test_a_payload_that_is_not_an_array_is_a_failed_batch(spec):
    with pytest.raises(ValueError, match="expected a JSON array"):
        parse_rows({"en": "Exit", "ka": "გასასვლელი"}, spec)


def test_mtavruli_is_folded_at_parse_time_not_dropped(spec):
    rows, _ = parse_rows([{"en": "Exit", "ka": "ᲒᲐᲡᲐᲡᲕᲚᲔᲚᲘ"}], spec)
    assert rows[0].target == "გასასვლელი"
    assert fold(spec, "ᲐᲠᲘᲡ") == "არის"


# --------------------------------------------------------------------- gates


def test_a_good_pair_passes(spec):
    assert check(spec, "No lifeguard on duty", "მაშველი არ არის") is None


def test_english_left_inside_the_georgian_is_rejected(spec):
    assert check(spec, "Emergency exit is on the left",
                 "Emergency გასასვლელი მარცხნივ არის") is GateReason.LATIN_LEAK


def test_allowlisted_latin_units_and_brands_survive(spec):
    assert check(spec, "Enter the Wi-Fi password", "შეიყვანეთ Wi-Fi პაროლი") is None
    assert check(spec, "USB port", "USB პორტი", band="w01_03") is None


def test_georgian_letters_in_the_english_side_are_rejected(spec):
    assert check(spec, "Exit გასასვლელი", "გასასვლელი") is GateReason.TARGET_IN_EN


def test_a_target_side_that_is_not_georgian_is_rejected(spec):
    assert check(spec, "Beware of the dog", "Attenti al cane") is GateReason.SCRIPT


def test_a_foreign_script_fragment_inside_the_georgian_is_rejected(spec):
    """Both of these reached a build: the share test passes a line that is mostly
    Georgian, and a stray CJK or Arabic run is a small fraction of one."""
    assert check(spec, "Apply directly to the skin", "直接 კანზე წაუსვით") is GateReason.SCRIPT
    assert check(spec, "Provides 15% of the reference intake",
                 "უზრუნველყოფს المرجენტული მოხმარების 15%-ს") is GateReason.SCRIPT
    # Latin is exempt from this check and judged by the allowlist instead, so an
    # allowlisted unit is still a pass rather than a foreign-script rejection.
    assert check(spec, "Capacity 250 ml", "ტევადობა 250 ml", band="w01_03") is None


def test_a_dropped_figure_is_rejected(spec):
    # The shipped student's own failure: "140 over 90" comes back without the 90.
    assert check(spec, "140 over 90", "140-ზე",
                 band="w01_03", numbers=NumberPolicy.REQUIRED) is GateReason.NUMBER_MISMATCH


def test_an_altered_figure_is_rejected_from_either_side(spec):
    assert check(spec, "Take 2 tablets 3 times a day",
                 "მიიღეთ 2 ტაბლეტი დღეში 4-ჯერ") is GateReason.NUMBER_MISMATCH
    assert check(spec, "Expires 2029", "ვარგისია 2019 წლამდე") is GateReason.NUMBER_MISMATCH


def test_a_figure_written_out_in_english_words_still_matches(spec):
    assert check(spec, "Open until five", "ღიაა 5 საათამდე") is None


def test_a_numeric_cell_refuses_a_row_with_no_figure(spec):
    assert check(spec, "Closed for lunch", "შესვენება",
                 numbers=NumberPolicy.REQUIRED) is GateReason.NUMBER_MISSING
    assert check(spec, "Closed for lunch", "შესვენება") is not GateReason.NUMBER_MISSING


def test_a_row_far_outside_its_band_is_rejected(spec):
    assert check(spec, "Please do not leave your luggage unattended anywhere inside the terminal building",
                 "გთხოვთ, ნუ დატოვებთ თქვენს ბარგს უყურადღებოდ ტერმინალის შენობაში") is GateReason.BAND
    # The slack keeps a row that missed the band by a word, which is common and
    # harmless; only the ones that answered a different question are dropped.
    assert check(spec, "Please keep your ticket until the end of the journey",
                 "შეინახეთ ბილეთი მგზავრობის დასრულებამდე", band="w09_18") is None


def test_one_side_truncated_to_nothing_is_rejected(spec):
    assert check(spec, "Do not mix with bleach and other cleaning products",
                 "არა") is GateReason.LENGTH_RATIO


def test_a_short_label_is_not_rejected_for_its_length(spec):
    """Both of these are real pairs the character-ratio version threw away, and
    the short band is the whole point of the set."""
    assert check(spec, "Cottage cheese", "ხაჭო", band="w01_03") is None
    assert check(spec, "No phones", "მობილური ტელეფონების გამოყენება აკრძალულია",
                 band="w01_03") is None


def test_identical_sides_are_rejected(spec):
    assert check(spec, "Wi-Fi", "Wi-Fi", band="w01_03") is GateReason.IDENTICAL


def test_tabs_and_newlines_are_rejected_because_the_tsv_cannot_hold_them(spec):
    assert check(spec, "Exit\there", "გასასვლელი") is GateReason.CONTROL


# ------------------------------------------------------- numbers, in the raw


@pytest.mark.parametrize("target,en", [
    ("ფასი 12,50 ლარი", "Price 12.50 GEL"),
    ("ვარგისია 15.03.2027-მდე", "Best before 15 March 2027"),
    ("მიიღეთ 500 მგ დღეში 2-ჯერ", "Take 500 mg twice a day"),
    ("პლატფორმა 3", "Platform three"),
])
def test_number_renderings_that_are_the_same_figure(target, en):
    assert numbers_agree(target, en)


@pytest.mark.parametrize("target,en", [
    ("სიჩქარე 60 კმ/სთ", "Speed limit 80 km/h"),
    ("4 პორცია", "Serves 6"),
    ("ღიაა 09:00-დან 18:00-მდე", "Open from 09:00"),
    ("ტემპერატურა 20 გრადუსი", "Temperature 20 degrees, humidity 60 percent"),
])
def test_number_renderings_that_are_not(target, en):
    assert not numbers_agree(target, en)


# ------------------------------------------------- dedup keys and exclusions


def test_norm_collapses_the_differences_that_are_not_differences():
    assert norm("Sign Out!") == norm("  sign   out ") == "sign out"
    assert norm("გასასვლელი.") == norm("გასასვლელი")
    assert norm("Exit") != norm("Exits")


def test_known_corpora_are_read_column_order_blind(tmp_path):
    en_first = tmp_path / "a.tsv"
    en_first.write_text("Exit\tგასასვლელი\n", encoding="utf-8")
    ka_first = tmp_path / "b.tsv"
    ka_first.write_text("შესასვლელი\tEntrance\n", encoding="utf-8")
    known = load_known([en_first, ka_first])
    assert {"exit", "გასასვლელი", "შესასვლელი", "entrance"} <= known


def test_eval_lines_are_held_out_by_digest_and_by_normalised_text(tmp_path):
    probe = tmp_path / "check.en"
    probe.write_text("Beware of dog\n\nSign out\n", encoding="utf-8")
    digests = tmp_path / "eval_exclude.sha256"
    digests.write_text(sha("Dead end") + "\n", encoding="utf-8")
    ex_digests, ex_text = load_exclusions([probe, digests])
    assert sha("Dead end") in ex_digests
    assert sha("Beware of dog") in ex_digests
    # The leak this guards against was case- and punctuation-shifted copies of
    # the check set, which a digest of the raw line does not catch on its own.
    assert norm("BEWARE OF DOG!") in ex_text
    assert norm("sign out") in ex_text
