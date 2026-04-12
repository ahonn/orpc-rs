# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.0] - 2026-04-12

### Added

- Add native bracket notation support in OpenAPI query strings ([2652a48](https://github.com/ahonn/orpc-rs/commit/2652a48f6ca09da8ddcdf56ac859aa89d0c35ce0))

### Fixed

- Address path prefix boundary, multipart index, and SSE parsing issues ([25ca728](https://github.com/ahonn/orpc-rs/commit/25ca7285a8882171c83f6a0fea12ef215b9f05bb))
- Remove unused serde_urlencoded and harden query string parsing ([0cc9eb6](https://github.com/ahonn/orpc-rs/commit/0cc9eb6b4d5c135d299de570153e9f488689214c))

### Other

- Update CLAUDE.md, clean up stale comments, add missing READMEs ([ecd9ada](https://github.com/ahonn/orpc-rs/commit/ecd9ada1c1605a36780201675be6481b612a42b5))
- Add serde_qs dependency for bracket notation support ([5225efa](https://github.com/ahonn/orpc-rs/commit/5225efa93ad64e35b9856a2fdcfa9fc275481f68))


## [1.0.0] - 2026-03-30

### Added

- Add file upload, Rust client, and #[orpc_service] proc-macro ([4a44ae5](https://github.com/ahonn/orpc-rs/commit/4a44ae5a659f3ffe9c27563637daf03b9c4fc6b3))

### Fixed

- Address resource leak and safety issues inspired by rspc issues ([b20464f](https://github.com/ahonn/orpc-rs/commit/b20464f416e7f7340018ffbf4890e8488c2948f5))

### Other

- Prepare v1.0.0 release ([3114e53](https://github.com/ahonn/orpc-rs/commit/3114e53abd8338d0a5e2ecc7f434ea6ef313956f))
- Release v0.1.2 ([099d643](https://github.com/ahonn/orpc-rs/commit/099d643faa5b5e8c6c529623006dfa8716b64e2a))

