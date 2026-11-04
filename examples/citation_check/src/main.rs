//! This is a port of the TypeSafe cookbook <https://docs.typesafe.ai/cookbooks/citation_check>.
//! It checks if eight citations about JSON Web Tokens are true to RFC 7519, the source that they
//! cite.
//!
//! Each citation makes a claim, and most of them also quote the RFC. The program checks each
//! citation in two steps:
//!
//! 1. Find the quote in the RFC. If the quote is not there, the citation is fabricated, and no
//!    model is necessary. A citation with no quote names its section.
//! 2. Ask Jev if that section supports the claim, contradicts it, or says nothing about it.
//!
//! The program accepts a verdict with a high confidence. A person reviews each other verdict.

use regex::Regex;
use serde::Deserialize;
use serde_json::json;
use std::error::Error;
use system2::Client;
use system2::prelude::*;

const MODEL: &str = "jev-1.14.0";

/// The lowest confidence that accepts a verdict with no review by a person.
const AUTO_ACCEPT_THRESHOLD: f64 = 0.8;

const RFC: &str = include_str!("../data/rfc7519.txt");
const CITATIONS: &str = include_str!("../data/citations.json");

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    Client::from_env()?.model(MODEL).set_global();

    let source = load_source(RFC)?;
    let sections = split_sections(&source)?;
    let citations: Vec<Citation> = serde_json::from_str(CITATIONS)?;

    let header = row("citation", "verdict", "conf", "action");
    println!("{header}\n{}", "-".repeat(header.len()));
    for citation in &citations {
        let Some(section) = find_section(&sections, citation) else {
            // a quote that is not in the RFC needs no model
            println!("{}", row(&citation.id, "fabricated", "-", "auto"));
            continue;
        };

        let answer = choice!("How does the section relate to the claim?", Relation)
            .ask(&json!({
                "claim": citation.claim,
                "section": section,
            }))
            .await?;

        let relation = answer.most_likely();
        let confidence = answer.confidence();
        let action = if confidence >= AUTO_ACCEPT_THRESHOLD {
            "auto"
        } else {
            "review"
        };

        let verdict = relation.verdict();
        let confidence = format!("{confidence:.2}");
        println!("{}", row(&citation.id, verdict, &confidence, action));
    }

    Ok(())
}

#[derive(Deserialize)]
struct Citation {
    id: String,
    claim: String,
    quote: Option<String>,
    section: String,
}

struct Section<'s> {
    number: &'s str,
    text: &'s str,
}

#[derive(Options)]
enum Relation {
    /// The section states the claim or directly implies that it is true
    Supports,
    /// The section states the opposite of the claim or implies it is false
    Contradicts,
    /// The section does not address what the claim asserts, either way
    SaysNothing,
}

impl Relation {
    fn verdict(&self) -> &'static str {
        match self {
            Relation::Supports => "verified",
            Relation::Contradicts => "contradicted",
            Relation::SaysNothing => "unsupported",
        }
    }
}

/// Removes the page headers and footers from the RFC, because they break its paragraphs. Then
/// removes the extra blank lines that they leave.
fn load_source(rfc: &str) -> Result<String, regex::Error> {
    let footer = Regex::new(r"^Jones, et al\.\s.*\[Page \d+\]$")?;
    let header = Regex::new(r"^RFC 7519\s+JSON Web Token \(JWT\)\s+May 2015$")?;
    let lines: Vec<&str> = rfc
        .lines()
        .map(|line| line.trim_start_matches('\x0c'))
        .filter(|line| !footer.is_match(line) && !header.is_match(line))
        .collect();
    let blank_lines = Regex::new(r"\n{3,}")?;
    Ok(blank_lines
        .replace_all(&lines.join("\n"), "\n\n")
        .into_owned())
}

/// Splits the RFC into its numbered sections, such as "4.1.3", at their header lines.
fn split_sections(source: &str) -> Result<Vec<Section<'_>>, regex::Error> {
    let boundary = Regex::new(r"(?m)^(?:(\d+(?:\.\d+)*)\.  .+|Appendix [A-Z]\..*)$")?;
    let marks: Vec<_> = boundary.captures_iter(source).collect();
    let mut sections = Vec::new();
    for (index, mark) in marks.iter().enumerate() {
        // an appendix header ends the section before it, but starts no numbered section
        let Some(number) = mark.get(1) else {
            continue;
        };
        let start = mark.get_match().start();
        let end = marks
            .get(index + 1)
            .map_or(source.len(), |next| next.get_match().start());
        sections.push(Section {
            number: number.as_str(),
            text: source[start..end].trim(),
        });
    }
    Ok(sections)
}

/// Changes curly quotes to straight quotes, and each run of whitespace to one space. Then a
/// quote matches the source across line wraps.
fn normalize(text: &str) -> String {
    let folded = text.replace(['“', '”'], "\"").replace(['‘', '’'], "'");
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Finds the section that contains the quote of the citation. If the citation has no quote, finds
/// the section that it names.
fn find_section<'s>(sections: &[Section<'s>], citation: &Citation) -> Option<&'s str> {
    let section = match &citation.quote {
        Some(quote) => {
            let quote = normalize(quote);
            sections
                .iter()
                .find(|section| normalize(section.text).contains(&quote))
        }
        None => sections
            .iter()
            .find(|section| section.number == citation.section),
    };
    section.map(|section| section.text)
}

fn row(citation: &str, verdict: &str, confidence: &str, action: &str) -> String {
    format!("{citation:<18}{verdict:<14}{confidence:>6}  {action}")
}
