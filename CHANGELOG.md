# Changelog

## 0.2.0

- Default to author–year citation keys with a stable eight-digit DOI hash.
- Add `--key-style doi` to preserve v0.1 citation keys when regenerating entries.
- Handle bare month names such as `month=June` in DOI-provided BibTeX.

## 0.1.0

- Resolve a DOI or DOI URL and write one Hayagriva bibliography entry to stdout.
- Generate stable, Typst-compatible citation keys from DOIs.
- Support appending entries to a shared YAML bibliography.
