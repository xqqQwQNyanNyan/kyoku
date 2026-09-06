---
language:
- en
license: agpl-3.0
library_name: pytorch
tags:
- riichi-mahjong
- mahjong-ai
- mortal
- dqn
- four-player
pipeline_tag: other
---

# Mortal V4 582500 — Four-player Riichi Mahjong

This repository publishes the `mortal-hpc` four-player checkpoint used by the
Akagi integration. It is a Mortal V4 checkpoint at **582,500 training steps**.
The checkpoint is intended for four-player riichi mahjong inference and is not
a standalone executable: use it with a compatible Mortal V4 runtime.

## What is included

- `mortal_582500.pth` — the PyTorch checkpoint.
- `model-manifest.json` — release identity, geometry, and SHA-256 digests.
- `Mortal-LICENSE` — upstream Mortal license and attribution.

The release contract is:

| Property | Value |
| --- | --- |
| Model family | Mortal V4 |
| Players | 4 |
| Checkpoint steps | 582,500 |
| Observation shape | `[1012, 34]` |
| Legal action space | 46 |
| DQN output width | 47 (46 actions plus value head) |
| Checkpoint SHA-256 | `738e0d6e3c0ce9671629554ad39abd147d2ffbac676e80b194c83f2acc0fea20` |

## What is improved relative to the older release path?

This is a **continuation checkpoint**, not a new network architecture. The
main model-level difference is the later training state (582,500 steps) while
keeping the Mortal V4 interface and geometry compatible with the existing
runtime.

The accompanying release work also improves reproducibility and deployment:

1. The exact checkpoint is pinned by SHA-256 rather than selected by a mutable
   filename or a silent fallback.
2. The manifest records the model version, observation shape, action space, and
   runtime digest.
3. The Akagi adapter performs a preflight identity check and deterministic legal
   replay check before automatic ranking.
4. Four-player Mortal and the existing three-player/native rollback paths stay
   explicitly separated.

These are release and integration improvements; they should not be interpreted
as proof of a universal strength increase.

## Evaluation evidence

The current 582,500-step checkpoint has passed local identity/loading checks and
a deterministic legal-replay check. A direct large-sample head-to-head strength
evaluation of this exact checkpoint is not included yet.

For context, earlier internal HPC continuation checkpoints were evaluated under
matched fixed seeds. These results are historical ablations, not a guarantee for
the 582,500-step release:

| Earlier comparison | Games | Average rank | Average points |
| --- | ---: | ---: | ---: |
| 20k continuation vs Mortal V4 298k baseline | 256 per side | 2.410 vs 2.520 | +7.91 vs -2.99 |
| 20k continuation vs Akagi native 4p | 80 per side | 2.150 vs 2.888 | +24.75 vs -27.56 |

The tests were limited in size and use fixed seeds. They suggest that the HPC
continuation path can improve the selected metrics in those matchups, but they
are not statistically sufficient to claim superiority over every other Mortal,
Akagi, or Mahjong Soul setup. Small quick tests were mixed, so users should run
their own evaluation on a fresh seed set.

## Verification

The manifest should report:

```text
model_id              mortal-hpc
players               4
checkpoint_steps      582500
observation_shape     [1012, 34]
action_space          46
mortal_version        4
```

The checkpoint can be verified by comparing its SHA-256 with the value in
`model-manifest.json`. For inference, use the upstream Mortal V4 model/runtime
and the four-player `libriichi` action encoding described by the upstream
project.

## Provenance and license

This release is based on the Mortal V4 project by Equim-chan:

- Upstream repository: https://github.com/Equim-chan/Mortal
- License: AGPL-3.0-or-later; see `Mortal-LICENSE`.

Permission to redistribute this checkpoint has been confirmed by the repository
owner for this publication. Please preserve the attribution and license when
redistributing or integrating the model.

## Limitations

- This is four-player only; it is not a three-player model.
- The checkpoint is not a replacement for the complete Mortal runtime.
- The historical comparisons above do not establish a statistically significant
  Elo or online-rank advantage.
- Automated online play may be restricted by the relevant platform terms of
  service; users are responsible for their own use.
