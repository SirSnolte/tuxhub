# Contributing

Thank you for your interest in tuxly.

## Reporting problems

Open an issue and include:

- the tuxly version (Settings, About),
- the distribution and desktop environment,
- the steps to reproduce the problem and what you expected instead.

## Proposing workshop entries

Open an issue with the repository of the tool, its source (GitHub release or
Flathub) and a short description. See
[docs/tool-development.md](docs/tool-development.md) for the format.

## Pull requests

1. Discuss larger changes in an issue first.
2. Keep pull requests focused on one change.
3. Make sure the checks pass:

   ```bash
   cargo fmt --all --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   npm run check
   ```

4. User-facing text belongs in `src/lib/i18n.svelte.ts` and needs an English
   and a German translation.

By contributing, you agree that your contributions are licensed under the
MIT License.
