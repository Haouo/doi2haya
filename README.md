# doi2haya

Convert a DOI into a [Hayagriva](https://github.com/typst/hayagriva) YAML entry
for a Typst bibliography. The command writes YAML to stdout and diagnostics to
stderr. It requires `curl` on your PATH and a network connection to resolve
DOI metadata.

## Install

From the source checkout:

```sh
cargo install --locked --path .
```

## Usage

```sh
doi2haya 10.1126/science.169.3946.635
doi2haya 10.1126/science.169.3946.635 > refs.yml
doi2haya https://doi.org/10.1038/171737a0 >> refs.yml
```

The command accepts a bare DOI, a resolver URL, or a DOI prefixed with `doi:`.
It requests BibTeX from doi.org and converts the available metadata with the
Hayagriva Rust library. If the resolver cannot supply a BibTeX record, the
command exits with an error.

Citation keys are derived from the DOI. Cite the same DOI using the same key,
and append each DOI only once per bibliography.

```typst
See @doi-10.1126_2fscience.169.3946.635.

#bibliography("refs.yml")
```

## License

[MIT](LICENSE)
