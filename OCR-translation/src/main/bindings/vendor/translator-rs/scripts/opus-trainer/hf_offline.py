"""Load a baked HF tokenizer from its local cache, never the network.

transformers 4.57's tokenizer loader runs `_patch_mistral_regex`, which for a
REMOTE model id calls `huggingface_hub.model_info()` over the network and — this
is the trap — ignores HF_HUB_OFFLINE. During an HF 504 (observed 2026-07-16)
that call hard-fails, so `AutoTokenizer.from_pretrained("facebook/nllb-...")`
dies even though every file it needs is already in the image. The guard that
skips the network call is `_is_local`, set only when the path is a local dir, so
loading from the cached snapshot DIR dodges it entirely.

nllb-ct2 trims the >100M weight blobs at build, which can leave a second,
incomplete snapshot dir behind; pick the one that actually holds tokenizer.json
(the fast tokenizer — protobuf for the slow one isn't installed).
"""

from __future__ import annotations

import glob
import os

from transformers import AutoTokenizer


def local_snapshot(model: str, hf_home: str | None = None) -> str:
    """The local cache dir for `model` containing tokenizer.json, or `model`
    itself if none is cached (letting the caller fall back to a network load)."""
    home = hf_home or os.environ.get("HF_HOME", "/opt/hf")
    snaps = os.path.join(home, "hub", "models--" + model.replace("/", "--"), "snapshots")
    for d in sorted(glob.glob(os.path.join(snaps, "*"))):
        if os.path.exists(os.path.join(d, "tokenizer.json")):
            return d
    return model


def load_tokenizer(model: str, src_lang: str | None = None) -> AutoTokenizer:
    tok = AutoTokenizer.from_pretrained(local_snapshot(model))
    if src_lang is not None:
        tok.src_lang = src_lang
    return tok
