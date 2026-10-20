from __future__ import annotations

import stat
from pathlib import Path

import pytest

from pipe.artstore import (
    Aligned,
    ArtifactType,
    ArtStore,
    EvalSet,
    Filtered,
    Raw,
    Trained,
    Vocab,
    typed_stage,
)
from pipe.config import StoreDests
from pipe.ledger import Ledger
from pipe.step import Run
from pipe.store import Store
from pipe.types import Digest, Kind, RunId, digest_file


@pytest.fixture
def art(tmp_path: Path) -> ArtStore:
    return ArtStore(
        tmp_path / "root",
        StoreDests(raw_dest=tmp_path / "storage", default_dest=tmp_path / "nvme"),
    )


def corpus(tmp_path: Path, name: str = "corpus.txt", text: str = "a\nb\nc\n") -> Path:
    p = tmp_path / name
    p.write_text(text)
    return p


def test_publish_get_roundtrip(art: ArtStore, tmp_path: Path) -> None:
    src = corpus(tmp_path)
    published = art.publish(src, ArtifactType.RAW, label="my Corpus v1")
    assert isinstance(published, Raw)
    assert published.digest == digest_file(src)
    assert published.lines == 3
    assert published.path.parent == art.dests.raw_dest / "dataset/raw"
    assert published.path.name.endswith("-my-corpus-v1")
    assert stat.S_IMODE(published.path.stat().st_mode) == 0o444

    got = art.get(published.digest)
    assert isinstance(got, Raw)
    assert got.stored == published.stored


def test_publish_default_dest_and_idempotency(art: ArtStore, tmp_path: Path) -> None:
    src = corpus(tmp_path)
    first = art.publish(src, ArtifactType.FILTERED)
    assert first.path.parent == art.dests.default_dest / "dataset/filtered"
    again = art.publish(src, ArtifactType.FILTERED)
    assert again.stored == first.stored


def test_publish_refuses_digest_collision_with_different_meta(
    art: ArtStore, tmp_path: Path
) -> None:
    src = corpus(tmp_path)
    art.publish(src, ArtifactType.FILTERED)
    with pytest.raises(ValueError, match="different metadata"):
        art.publish(src, ArtifactType.RAW)
    with pytest.raises(ValueError, match="different metadata"):
        art.publish(src, ArtifactType.FILTERED, label="renamed")


def test_publish_without_store_config_is_hard_error(tmp_path: Path) -> None:
    bare = ArtStore(tmp_path / "root", None)
    with pytest.raises(ValueError, match=r"\[store\]"):
        bare.publish(corpus(tmp_path), ArtifactType.RAW)


def test_model_must_go_through_publish_model(art: ArtStore, tmp_path: Path) -> None:
    model = corpus(tmp_path, "model.npz", "weights")
    with pytest.raises(ValueError, match="publish_model"):
        art.publish(model, ArtifactType.TRAINED)


def test_publish_model_refuses_empty_scores(art: ArtStore, tmp_path: Path) -> None:
    model = corpus(tmp_path, "model.npz", "weights")
    with pytest.raises(ValueError, match="without Scores"):
        art.publish_model(model, ArtifactType.TRAINED, scores=[])


def test_publish_model_refuses_unrelated_score(art: ArtStore, tmp_path: Path) -> None:
    model = corpus(tmp_path, "model.npz", "weights")
    evalset = art.publish(corpus(tmp_path, "flores.dev"), ArtifactType.EVALSET)
    assert isinstance(evalset, EvalSet)
    other = Digest("ab" * 32)
    stray = art.publish_score(other, evalset, "chrf++", 1.0)
    with pytest.raises(ValueError, match="does not descend"):
        art.publish_model(model, ArtifactType.TRAINED, scores=[stray])


def test_publish_model_happy_path(art: ArtStore, tmp_path: Path) -> None:
    model_file = corpus(tmp_path, "model.npz", "weights")
    model_digest = digest_file(model_file)
    evalset = art.publish(corpus(tmp_path, "flores.dev"), ArtifactType.EVALSET)
    assert isinstance(evalset, EvalSet)
    scores = [
        art.publish_score(model_digest, evalset, metric, value)
        for metric, value in (("chrf++", 37.53), ("comet22", 77.25))
    ]
    model = art.publish_model(
        model_file, ArtifactType.TRAINED, scores=scores, label="uig-r2"
    )
    assert isinstance(model, Trained)
    table = art.score_table(model=model.digest)
    assert {(r["metric"], r["value"]) for r in table} == {("chrf++", 37.53), ("comet22", 77.25)}
    assert all(r["evalset_label"] is None for r in table)


def test_alias_set_resolve_list(art: ArtStore, tmp_path: Path) -> None:
    published = art.publish(corpus(tmp_path), ArtifactType.FILTERED)
    art.alias_set("uig/latest", published.digest)
    assert art.alias_resolve("uig/latest") == published.digest
    assert art.alias_list()[0]["alias"] == "uig/latest"
    with pytest.raises(KeyError, match="no alias"):
        art.alias_resolve("uig/nope")
    with pytest.raises(KeyError, match="no artifact"):
        art.alias_set("uig/bad", Digest("cd" * 32))


def test_resolve_prefix(art: ArtStore, tmp_path: Path) -> None:
    published = art.publish(corpus(tmp_path), ArtifactType.FILTERED)
    assert art.resolve(str(published.digest)[:12]) == published.digest
    with pytest.raises(KeyError):
        art.resolve("0123456789ab")


def test_pin_roundtrip_via_run(art: ArtStore, tmp_path: Path) -> None:
    published = art.publish(corpus(tmp_path), ArtifactType.FILTERED)
    art.alias_set("uig/latest", published.digest)
    pinned_digest = art.pin("uigen_r2", "hplt_src", "uig/latest")
    assert pinned_digest == published.digest
    assert art.pins("uigen_r2") == {"hplt_src": str(published.digest)}

    run = Run(
        id=RunId("t1"),
        store=Store(tmp_path / "root"),
        ledger=Ledger(tmp_path / "root" / "ledger.json"),
        scripts=tmp_path,
        flow="uigen_r2",
        pins=art.pins("uigen_r2"),
        art=art,
    )
    got = run.pinned("hplt_src")
    assert isinstance(got, Filtered)
    assert got.digest == published.digest

    with pytest.raises(KeyError, match=r"pipe pin uigen_r2 vocab <alias\|digest>"):
        run.pinned("vocab")


def test_typed_stage_rejects_wrong_wrapper(art: ArtStore, tmp_path: Path) -> None:
    raw = art.publish(corpus(tmp_path, "a.txt"), ArtifactType.RAW)
    filtered = art.publish(corpus(tmp_path, "b.txt", "x\n"), ArtifactType.FILTERED)

    @typed_stage
    def decode(corpus_in: Filtered, known_good: Filtered | Aligned | None = None) -> str:
        return str(corpus_in.digest)

    assert decode(filtered) == str(filtered.digest)
    assert decode(filtered, known_good=None) == str(filtered.digest)
    with pytest.raises(TypeError, match="takes corpus_in: Filtered, got Raw"):
        decode(raw)
    with pytest.raises(TypeError, match="known_good"):
        decode(filtered, known_good=raw)

    @typed_stage
    def mixed(vocab: str | Raw) -> str:
        return "ok"

    assert mixed("plain") == "ok"
    assert mixed(raw) == "ok"
    with pytest.raises(TypeError, match="vocab"):
        mixed(filtered)


def test_evalset_is_outside_the_dataset_family(art: ArtStore, tmp_path: Path) -> None:
    evalset = art.publish(corpus(tmp_path, "flores.dev"), ArtifactType.EVALSET)

    @typed_stage
    def filter_stage(corpus_in: Raw) -> None:
        raise AssertionError("must not be reached")

    with pytest.raises(TypeError):
        filter_stage(evalset)


def test_publish_kind_override_skips_line_count(art: ArtStore, tmp_path: Path) -> None:
    vocab = corpus(tmp_path, "vocab.spm", "binary-ish\n")
    published = art.publish(vocab, ArtifactType.RAW, kind=Kind.BLOB)
    assert published.lines is None


def test_vocab_is_its_own_family(art: ArtStore, tmp_path: Path) -> None:
    corpus_art = art.publish(corpus(tmp_path), ArtifactType.FILTERED)
    spm = tmp_path / "vocab.spm"
    spm.write_bytes(b"\x00spm\x01binary")
    published = art.publish(spm, ArtifactType.VOCAB, parents=(corpus_art.digest,))
    assert isinstance(published, Vocab)
    assert published.lines is None
    assert published.stored.parents == (corpus_art.digest,)

    @typed_stage
    def train_stage(train_tsv: Aligned, vocab: Vocab) -> None:
        pass

    aligned = art.publish(corpus(tmp_path, "train.tsv", "a\tb\n"), ArtifactType.ALIGNED)
    with pytest.raises(TypeError, match="vocab: Vocab"):
        train_stage(train_tsv=aligned, vocab=corpus_art)  # type: ignore[arg-type]
