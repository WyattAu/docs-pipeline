# Changelog

All notable changes to this project are documented here. Format: [Keep a
Changelog](https://keepachangelog.com/) — versions follow [semver](https://semver.org).

## [Unreleased]

## [0.1.2] - 2026-09-09

### Changed
- Replace production `unwrap()` calls on static `LazyLock<Regex>`
  initializers with a `static_regex()` helper using
  `.expect("validated static regex pattern")`, documenting the
  INVARIANT that these patterns are infallible by construction
  (satisfies `clippy::unwrap_used` under `-D warnings`).

## [0.1.1] - 2026-09-09

### Changed
- Fuzz harnesses for markdown render/highlight/sanitize pipeline and
  repo metadata normalization.

## [0.1.0] - 2026-09-05

### Added
- Initial public release.
