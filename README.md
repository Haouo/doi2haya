# doi2haya

Convert a DOI into a [Hayagriva](https://github.com/typst/hayagriva) YAML entry
for a Typst bibliography. The command writes YAML to stdout and diagnostics to
stderr. It requires `curl` on your PATH and a network connection to resolve
DOI metadata.

## Install

From crates.io:

```sh
cargo install --locked doi2haya
```

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

By default, citation keys combine the first author's surname, publication year,
and an eight-digit hexadecimal hash of the DOI. For example:

```typst
@Frank1970-83ee4c03

#bibliography("refs.yml")
```

The suffix is computed from the DOI regardless of ASCII letter case, so the
same DOI and metadata produce the same key independently of append order.
Missing author or year metadata uses `Anon` or `nd`, respectively. The short
hash reduces collisions between works with the same author and year. Append
each DOI only once per bibliography.

To keep the DOI-based keys used in v0.1, choose `--key-style doi`:

```sh
doi2haya --key-style doi 10.1126/science.169.3946.635
```

This produces `doi-10.1126_2fscience.169.3946.635`. Use this mode when
regenerating entries already cited by their v0.1 keys.

## License

[MIT](LICENSE)
