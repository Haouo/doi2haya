use std::{
    env,
    error::Error,
    fmt::Write as _,
    io::{self, Write as _},
    process::{Command, ExitCode},
};

use biblatex::{Bibliography, PermissiveType, RawBibliography, RawChunk};
use hayagriva::{Entry, Library, io::to_yaml_str};
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

const USAGE: &str = "Usage: doi2haya [--key-style <author-year|doi>] <doi>";

#[derive(Clone, Copy)]
enum KeyStyle {
    AuthorYear,
    Doi,
}

impl KeyStyle {
    fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value {
            "author-year" => Ok(Self::AuthorYear),
            "doi" => Ok(Self::Doi),
            _ => Err(format!("unknown key style {value:?}; expected author-year or doi").into()),
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("doi2haya: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| "arguments must be valid UTF-8")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    let (input, style) = match args.as_slice() {
        ["-h" | "--help"] => {
            println!("{USAGE}\nDefault key style: author-year");
            return Ok(());
        }
        ["-V" | "--version"] => {
            println!("doi2haya {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        [input] => (*input, KeyStyle::AuthorYear),
        ["--key-style", style, input] | [input, "--key-style", style] => {
            (*input, KeyStyle::parse(style)?)
        }
        _ => return Err(USAGE.into()),
    };

    let doi = parse_doi(input)?;
    let bibtex = fetch_bibtex(&doi)?;
    let yaml = convert(&doi, &bibtex, style)?;

    // A leading newline also makes appending to files without a final newline safe.
    writeln!(io::stdout().lock(), "\n{}", yaml.trim_end_matches('\n'))?;
    Ok(())
}

fn parse_doi(input: &str) -> Result<String, Box<dyn Error>> {
    let input = input.trim();
    let resolver_path = [
        "https://doi.org/",
        "http://doi.org/",
        "https://dx.doi.org/",
        "http://dx.doi.org/",
    ]
    .into_iter()
    .find_map(|prefix| input.strip_prefix(prefix));

    let doi = match resolver_path {
        Some(path) => {
            let path = path.split(['?', '#']).next().unwrap_or_default();
            percent_decode_str(path).decode_utf8()?.into_owned()
        }
        None => input
            .strip_prefix("doi:")
            .unwrap_or(input)
            .trim()
            .to_owned(),
    };

    let valid = doi.split_once('/').is_some_and(|(prefix, suffix)| {
        prefix.strip_prefix("10.").is_some_and(|registrant| {
            !registrant.is_empty()
                && registrant
                    .split('.')
                    .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        }) && !suffix.is_empty()
    });
    if !valid || doi.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return Err("expected a DOI such as 10.1126/science.169.3946.635".into());
    }
    Ok(doi)
}

fn fetch_bibtex(doi: &str) -> Result<String, Box<dyn Error>> {
    // Encode DOI components so punctuation in a DOI cannot become URL syntax.
    const DOI_PATH_ENCODING: &percent_encoding::AsciiSet = &NON_ALPHANUMERIC
        .remove(b'.')
        .remove(b'-')
        .remove(b'_')
        .remove(b'/');
    let path = utf8_percent_encode(doi, DOI_PATH_ENCODING);
    let url = format!("https://doi.org/{path}");
    let output = Command::new("curl")
        .args([
            "--disable",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-redirs",
            "10",
            "--max-time",
            "30",
            "--path-as-is",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--header",
            "Accept: application/x-bibtex",
            "--user-agent",
            concat!("doi2haya/", env!("CARGO_PKG_VERSION")),
            &url,
        ])
        .output()
        .map_err(|error| format!("could not run curl: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!("could not retrieve BibTeX for {doi}: {}", detail.trim()).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn convert(doi: &str, bibtex: &str, style: KeyStyle) -> Result<String, Box<dyn Error>> {
    let mut raw = RawBibliography::parse(bibtex)?;
    normalize_bare_months(&mut raw);
    let bibliography = Bibliography::from_raw(raw)?;
    let mut entries = bibliography.iter();
    let Some(record) = entries.next() else {
        return Err("DOI resolver returned no BibTeX entry".into());
    };
    if entries.next().is_some() {
        return Err("DOI resolver returned multiple BibTeX entries".into());
    }

    let mut record = record.clone();
    record.key = match style {
        KeyStyle::AuthorYear => author_year_key(doi, &record),
        KeyStyle::Doi => doi_key(doi),
    };
    let mut entry = Entry::try_from(&record)?;
    if entry.doi().is_none() {
        entry.set_doi(doi.to_owned());
    }
    let mut library = Library::new();
    library.push(&entry);
    Ok(to_yaml_str(&library)?)
}

// Some DOI providers emit bare month names such as `month=June`.
fn normalize_bare_months(bibliography: &mut RawBibliography<'_>) {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];

    for entry in &mut bibliography.entries {
        for field in &mut entry.v.fields {
            if !field.key.v.eq_ignore_ascii_case("month") {
                continue;
            }
            let [chunk] = field.value.v.as_mut_slice() else {
                continue;
            };
            let RawChunk::Abbreviation(name) = chunk.v else {
                continue;
            };
            if bibliography
                .abbreviations
                .iter()
                .any(|item| item.key.v == name)
            {
                continue;
            }
            if let Some(month) = MONTHS.iter().find(|month| month.eq_ignore_ascii_case(name)) {
                chunk.v = RawChunk::Normal(month);
            }
        }
    }
}

fn author_year_key(doi: &str, record: &biblatex::Entry) -> String {
    let surname: String = record
        .author()
        .ok()
        .and_then(|authors| authors.into_iter().next())
        .map(|author| {
            author
                .name
                .chars()
                .filter(|ch| ch.is_alphanumeric())
                .collect()
        })
        .unwrap_or_default();
    let surname = if surname.is_empty() { "Anon" } else { &surname };
    let year = match record.date() {
        Ok(PermissiveType::Typed(date)) => hayagriva::types::Date::from(date).year.to_string(),
        _ => String::from("nd"),
    };

    // FNV-1a uses fixed constants so the suffix is stable across platforms and releases.
    let hash = doi.bytes().fold(0x811c9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte.to_ascii_lowercase())).wrapping_mul(0x01000193)
    });
    format!("{surname}{year}-{hash:08x}")
}

fn doi_key(doi: &str) -> String {
    let mut key = String::from("doi-");
    for byte in doi.bytes() {
        match byte {
            b'.' | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' => {
                key.push(char::from(byte.to_ascii_lowercase()));
            }
            b'-' => key.push('-'),
            _ => write!(key, "_{byte:02x}").expect("writing to a String cannot fail"),
        }
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_doi_inputs() {
        for input in [
            "10.1234/ab#c",
            " doi: 10.1234/ab#c ",
            "https://doi.org/10.1234/ab%23c?tracking=1",
        ] {
            assert_eq!(parse_doi(input).unwrap(), "10.1234/ab#c");
        }
        assert!(parse_doi("10.1234/").is_err());
        assert!(parse_doi("https://example.com/10.1234/abc").is_err());
    }

    #[test]
    fn converts_a_book_with_a_bare_month_name() {
        let bibtex = "@book{Nielsen_2012, title={Quantum Computation and Quantum Information},
            year={2012}, month=June, doi={10.1017/cbo9780511976667}}";
        let yaml = convert("10.1017/CBO9780511976667", bibtex, KeyStyle::Doi).unwrap();
        let library = hayagriva::io::from_yaml_str(&yaml).unwrap();
        let entry = library.get("doi-10.1017_2fcbo9780511976667").unwrap();

        assert_eq!(*entry.entry_type(), hayagriva::types::EntryType::Book);
        assert!(yaml.contains("date: 2012-06"));
    }

    #[test]
    fn author_year_keys_are_readable_and_stable() {
        let bibtex = "@article{example, author={García, Ana and Chang, Alex}, date={2026-01}}";
        let key = |doi| {
            let yaml = convert(doi, bibtex, KeyStyle::AuthorYear).unwrap();
            let library = hayagriva::io::from_yaml_str(&yaml).unwrap();
            library.keys().next().unwrap().to_owned()
        };
        assert_eq!(key("10.1234/A"), "García2026-506d23ea");
        assert_eq!(key("10.1234/a"), key("10.1234/A"));
        assert_ne!(key("10.1234/A"), key("10.1234/B"));

        let yaml = convert(
            "10.1234/a",
            "@misc{example, title={Title}}",
            KeyStyle::AuthorYear,
        )
        .unwrap();
        assert!(yaml.starts_with("Anonnd-506d23ea:"));
    }

    #[test]
    fn converts_and_appends_entries() {
        let bibtex = r#"@article{example,
            author = {Doe, Jane},
            title = {A title},
            journal = {A journal},
            year = {2024}
        }"#;
        let first = convert("10.1234/A", bibtex, KeyStyle::Doi).unwrap();
        let second = convert("10.1234/_2f", bibtex, KeyStyle::Doi).unwrap();
        let combined = format!("{}\n{second}", first.trim_end());
        let library = hayagriva::io::from_yaml_str(&combined).unwrap();

        assert_eq!(library.len(), 2);
        assert_eq!(
            library.get("doi-10.1234_2fa").unwrap().doi(),
            Some("10.1234/A")
        );
        assert!(library.get("doi-10.1234_2f_5f2f").is_some());
    }
}
