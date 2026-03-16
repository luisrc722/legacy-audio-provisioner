# ADR 0003: FFmpeg Normalization for Legacy Compatibility

- Status: Accepted
- Date: 2026-03-16
- Author: Luis / Legacy Audio Project

## 1. Context

Legacy car stereos fail with inconsistent codecs, unsupported metadata, VBR edge cases, and container complexity.

## 2. Decision

Normalize output through FFmpeg/ffprobe to produce deterministic, legacy-compatible MP3 output and stripped metadata.

## 3. Consequences

- Positive:
  - Stable playback on constrained legacy firmware.
  - Reduced variability between source catalogs.
- Negative:
  - Dependency on FFmpeg toolchain.
  - Additional CPU and I/O during normalization.
