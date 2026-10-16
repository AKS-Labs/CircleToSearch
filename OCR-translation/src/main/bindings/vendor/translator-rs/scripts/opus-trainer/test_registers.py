"""Mix arithmetic and the per-register filters — pure, so testable without a corpus."""

from __future__ import annotations

import pytest

from registers import Mix, Register, apply_extra

FULL = {Register.UI: 50_000, Register.HUMAN: 200_000,
        Register.DIALOGUE: 1_000_000, Register.ENTITY: 150_000}


def test_parse_roundtrip():
    mix = Mix.parse("ui=50000,human=200000,dialogue=1000000,entity=150000,crawl=fill", 10_000_000)
    assert mix.fill is Register.CRAWL
    assert mix.caps == FULL


def test_unassigned_register_is_refused():
    # UI omitted entirely — the exact shape of the bug this replaces, where a
    # register contributes nothing because nobody named it.
    with pytest.raises(ValueError, match="unassigned registers.*ui"):
        Mix.parse("human=200000,dialogue=1000000,entity=150000,crawl=fill", 10_000_000)


def test_fill_cannot_also_be_capped():
    with pytest.raises(ValueError, match="cannot also be capped"):
        Mix(total=10, fill=Register.CRAWL, caps={**FULL, Register.CRAWL: 5})


def test_caps_must_leave_room_for_fill():
    with pytest.raises(ValueError, match="leaving nothing"):
        Mix.parse("ui=50000,human=200000,dialogue=1000000,entity=150000,crawl=fill", 1_000_000)


def test_draw_takes_all_of_an_undersupplied_register():
    """Swahili's 10k UI lines all pass; they are not diluted and not padded."""
    mix = Mix(total=10_000_000, fill=Register.CRAWL, caps=FULL)
    taken = mix.draw({Register.UI: 9_963, Register.HUMAN: 40_000,
                      Register.DIALOGUE: 94_636, Register.ENTITY: 871_902,
                      Register.CRAWL: 50_000_000})
    assert taken[Register.UI] == 9_963          # under cap -> all of it
    assert taken[Register.ENTITY] == 150_000    # over cap -> capped
    assert sum(taken.values()) == 10_000_000
    assert taken[Register.CRAWL] == 10_000_000 - 9_963 - 40_000 - 94_636 - 150_000


def test_draw_does_not_invent_lines_the_pair_does_not_have():
    mix = Mix(total=10_000_000, fill=Register.CRAWL, caps=FULL)
    taken = mix.draw({r: 1_000 for r in Register})
    assert sum(taken.values()) == 5_000
    assert taken[Register.CRAWL] == 1_000


def test_ui_placeholder_lines_are_dropped_not_stripped():
    """Stripping leaves an ungrammatical HOLE on both sides: `%nation% have
    declared war` -> "the have declared war on us". 25.3% of raw en-tl UI lines
    carry a placeholder, so three quarters of the register survives the drop."""
    assert apply_extra(Register.UI, "Delete %d files", "Burahin ang %d na file") is None
    assert apply_extra(Register.UI, "The %nation% have declared war!",
                       "Nagdeklara ang %nation%!") is None


def test_ui_accelerators_are_stripped_not_dropped():
    """An accelerator removal leaves no hole, so these stay."""
    assert apply_extra(Register.UI, "_Save", "_I-save") == ("Save", "I-save")
    assert apply_extra(Register.UI, "Sa&ve", "I-sa&ve") == ("Save", "I-save")
    assert apply_extra(Register.UI, "View history", "Tingnan ang kasaysayan") == \
        ("View history", "Tingnan ang kasaysayan")


def test_ui_line_that_is_only_a_placeholder_is_dropped():
    assert apply_extra(Register.UI, "%s", "%s") is None
    assert apply_extra(Register.UI, "{0} {1}", "{0} {1}") is None


def test_entity_junk_identifiers_dropped():
    # Crawl usernames from the en->tl KD source, each one an example of a short
    # line passing through untranslated.
    assert apply_extra(Register.ENTITY, "SilkyCat3795", "SilkyCat3795") is None
    assert apply_extra(Register.ENTITY, "Studentin2024", "Studentin2024") is None
    assert apply_extra(Register.ENTITY, "Bath35137", "Bath35137") is None


def test_entity_real_names_kept():
    assert apply_extra(Register.ENTITY, "Douglas Road", "Kalye Douglas") == \
        ("Douglas Road", "Kalye Douglas")
    # camelCase is left alone on purpose — indistinguishable from real entities.
    for name in ("McDonald", "eBay", "DeVries", "NGdesign"):
        assert apply_extra(Register.ENTITY, name, name) == (name, name)


def test_entity_long_titles_dropped():
    long_title = " ".join(["word"] * 8)
    assert apply_extra(Register.ENTITY, long_title, long_title) is None


def test_dialogue_speaker_dash_stripped():
    assert apply_extra(Register.DIALOGUE, "- Get down!", "- Yumuko ka!") == \
        ("Get down!", "Yumuko ka!")


def test_registers_without_extras_pass_through():
    assert apply_extra(Register.CRAWL, "a b", "c d") == ("a b", "c d")
    assert apply_extra(Register.HUMAN, "a b", "c d") == ("a b", "c d")


def test_ui_printf_width_and_precision():
    """`%.1f EB` reached pool.ui on the first smoke run — a naive %[sdifgu] misses it."""
    assert apply_extra(Register.UI, "%.1f EB free", "%.1f EB libre") is None
    assert apply_extra(Register.UI, "Copied %02d of %02d", "Nakopya %02d ng %02d") is None


def test_ui_format_scaffolding_dropped():
    assert apply_extra(Register.UI, "%m/", "%m/") is None
    assert apply_extra(Register.UI, "%02d:%02d", "%02d:%02d") is None
    assert apply_extra(Register.UI, "%1$s", "%1$s") is None


def test_ui_mediawiki_braces():
    """translatewiki carries {VAR: x} and {{PLURAL:...}}, not just {0}."""
    assert apply_extra(Register.UI, "Avatar dimensions of ({VAR: img_info})",
                       "Mga sukat ng ({VAR: img_info})") is None
    assert apply_extra(Register.UI, "You have {{PLURAL:$1|one message|$1 messages}}",
                       "May {{PLURAL:$1|isang mensahe|$1 mensahe}} ka") is None


def test_ui_named_placeholder_forms_are_recognised():
    """These must MATCH so the line is dropped. When printf was tried first,
    `%language%` matched only `%la` and left "nguage%" as text — 312 lines of
    fluent-looking gibberish that no downstream check would have caught."""
    assert apply_extra(Register.UI, "Setting the language to %language%.",
                       "Itinatakda ang wika sa %language%.") is None
    assert apply_extra(Register.UI, "Removed %(count)d links",
                       "Tinanggal ang %(count)d link") is None


def test_ui_residual_format_char_drops_the_line():
    # Unhandled placeholder forms must not reach the student as literal text.
    assert apply_extra(Register.UI, "Set $wgEnableUploads to true", "Itakda ang $wgEnableUploads") is None
    assert apply_extra(Register.UI, "Progress: 50%", "Pag-usad: 50%") is None
