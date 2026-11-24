use std::{
    env,
    error::Error,
    fmt::Write as _,
    io::{self, Write as _},
    process::{Command, ExitCode},
};

use biblatex::Bibliography;
use hayagriva::{Entry, Library, io::to_yaml_str};
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

const USAGE: &str = "Usage: doi2haya <doi>";

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
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [input] = args.as_slice() else {
        return Err(USAGE.into());
    };
    let input = input.to_str().ok_or("DOI must be valid UTF-8")?;
    if matches!(input, "-h" | "--help") {
        println!("{USAGE}");
        return Ok(());
    }

    if matches!(input, "-V" | "--version") {
        println!("doi2haya {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let doi = parse_doi(input)?;
    let bibtex = fetch_bibtex(&doi)?;
    let yaml = convert(&doi, &bibtex)?;

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

fn convert(doi: &str, bibtex: &str) -> Result<String, Box<dyn Error>> {
    let bibliography = Bibliography::parse(bibtex)?;
    let mut entries = bibliography.iter();
    let Some(record) = entries.next() else {
        return Err("DOI resolver returned no BibTeX entry".into());
    };
    if entries.next().is_some() {
        return Err("DOI resolver returned multiple BibTeX entries".into());
    }

    let mut record = record.clone();
    record.key = citation_key(doi);
    let mut entry = Entry::try_from(&record)?;
    if entry.doi().is_none() {
        entry.set_doi(doi.to_owned());
    }
    let mut library = Library::new();
    library.push(&entry);
    Ok(to_yaml_str(&library)?)
}

fn citation_key(doi: &str) -> String {
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
    fn converts_and_appends_entries() {
        let bibtex = r#"@article{example,
            author = {Doe, Jane},
            title = {A title},
            journal = {A journal},
            year = {2024}
        }"#;
        let first = convert("10.1234/A", bibtex).unwrap();
        let second = convert("10.1234/_2f", bibtex).unwrap();
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
