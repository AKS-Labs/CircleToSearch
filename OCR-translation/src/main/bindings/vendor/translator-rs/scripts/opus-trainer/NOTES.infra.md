---
name: reference_distillation_infra
description: "Reusable compute, storage, and remote-execution reference for teacher-to-slimt distillation"
metadata:
  node_type: memory
  type: reference
  originSessionId: cfcd278d-eac4-4fc8-ba04-c981e1ad3340
---

This file contains stable infrastructure assumptions for the workflow in
[`NEW_LANGUAGE.md`](NEW_LANGUAGE.md). It does not contain active jobs, instance
IDs, dated measurements, or run-specific decisions.

## Machine roles

- **Local CPU-only host, if available:** data preparation, vocabulary training,
  alignment, conversion, and package checks. Use the project's uv-managed Python
  environment and Docker for tools that require an isolated system environment.
- **Remote GPU box:** teacher decoding, backward-model training, and student
  training. Use a CUDA image compatible with the selected stage.
- **Laptop:** final package inspection and application or catalog integration.

Keep large artifact transfers explicit and resumable. Pull verified outputs before
destroying a rented box.

## Runtime matrix

- Python preparation uses the project's uv-managed environment.
- Alignment and browsermt conversion run in Docker when their system libraries or
  binaries require isolation from the host.
- Student training uses the pinned Marian CUDA build and its CUDA 11.8 runtime.
  The supported fleet is Ada or an older CUDA 11.8-compatible architecture.
- NLLB CT2 decoding uses the pinned CT2 and Transformers environment from the
  corresponding container. Use streaming translation and a bounded batch-token
  setting.
- vLLM decoding requires a host with a compatible CUDA driver, including CUDA
  12.8 support when required by the image. Keep model and tokenizer availability
  local to the decoding environment so a runtime network outage cannot interrupt
  a paid shard.

Confirm the image, driver, model, tokenizer, and language-code configuration with
a smoke test before launching a full shard.

## Vast.ai selection and access

- Prefer verified offers in the region closest to the artifact source and model
  endpoints. Avoid regions with unreliable access to required services.
- Require direct SSH connectivity. Proxy transfer is unsuitable for large
  artifacts and can conceal a slow route until the GPU is idle.
- Filter offers by the CUDA version required by the image and by the GPU
  architecture supported by the Marian build.
- Attach the approved SSH key and wait for key propagation before staging files.
- Disable Vast.ai's automatic tmux wrapper with `~/.no_auto_tmux` before running
  non-interactive jobs.

Use the direct endpoint selected at rental time. Launch jobs with `nohup` and a
log file, then poll the log and process state. Keep SSH sessions short.

## Transfer and job lifecycle

1. Validate the offer, image, direct endpoint, and input size.
2. Stage only representative probe inputs for smoke tests.
3. Transfer large artifacts with a resumable, compressed method.
4. Launch the job and record its log and output paths.
5. Verify output completeness, row counts, ordering, and checksums.
6. Pull the verified outputs to the coordinating host.
7. Destroy the box and confirm that the lease is gone.

Freeze scripts, configurations, and input artifacts before a paid run. Treat a
change to any of them as a new run and verify that the pipeline identity includes
the changed arguments and configuration content.

## Paid-job checks

Before committing to a full shard, confirm:

- the container starts with the required driver and libraries
- the model and tokenizer load without network access when the image is intended
  to be self-contained
- the input is representative of the production corpus
- the output format preserves one result per source row
- GPU utilization and throughput are appropriate for the selected settings
- failure cleanup preserves partial outputs and releases the box

Do not leave an unattended paid job without a process monitor, a completion
marker, and a cleanup path.

## Bucket sync compares checksums, not sizes (2026-09-02)

`~/AndroidStudioProjects/bunny_sync.py` used to upload a file only when its size
differed from the remote listing. A catalog whose only change is a version label
(`lmt60_distill_20260901` → `…02`) keeps its byte count, so the ka-en publish on
2026-09-02 uploaded the pack files but not `index_v6.json`, and the CDN kept
serving the old catalog past the cache lifetime. Bunny's listing carries a
`Checksum` (SHA256, upper-case hex) per object; the sync now hashes size-equal
local files up to 1 MiB (`CHECKSUM_MAX_BYTES`) and uploads on a checksum
mismatch. Larger files are trusted on size alone, since a model pack that
changes at all changes size and hashing 14 GB of packs would dominate the run.
986 files totalling 96 MB fall under the limit; a no-op sync takes ~5 s. When a publish reports "live index does not mention the label",
check the remote checksum against the local file before blaming purge lag.
