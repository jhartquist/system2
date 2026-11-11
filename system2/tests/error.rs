//! Tests of the error types, and of their conversion into `system2::Error`.

mod support;

use std::path::Path;
use support::{INSTRUCTIONS, cache_path, client_for, mock_api, noul_response};
use system2::{
    AskError, Bbox, CacheError, Choice, Client, Error, Image, InvalidBbox, InvalidChoice,
    InvalidImage, InvalidJson, InvalidNoul, InvalidScore, Noul, NoulAnswer, QuestionError, Score,
};
use wiremock::MockServer;

const OPTIONS: &[&str] = &["spam", "ham"];
const LEVELS: &[&str] = &["low", "high"];
const REGION: &str = "The mug.";

type Questions = (Noul, Choice<&'static str>, Score<&'static str>, Bbox);

// a caller that builds each type of question from data
fn questions(
    rule: &str,
    options: &[&'static str],
    levels: &[&'static str],
    region: &str,
) -> Result<Questions, QuestionError> {
    Ok((
        Noul::new(rule)?,
        Choice::new(INSTRUCTIONS, options.to_vec())?,
        Score::new(INSTRUCTIONS, levels.to_vec())?,
        Bbox::new(region)?,
    ))
}

// the same caller, with the error of the crate
fn questions_or_error(
    rule: &str,
    options: &[&'static str],
    levels: &[&'static str],
    region: &str,
) -> system2::Result<Questions> {
    Ok((
        Noul::new(rule)?,
        Choice::new(INSTRUCTIONS, options.to_vec())?,
        Score::new(INSTRUCTIONS, levels.to_vec())?,
        Bbox::new(region)?,
    ))
}

fn questions_of_a_set() -> system2::Result<Questions> {
    Ok(questions("", OPTIONS, LEVELS, REGION)?)
}

fn image(bytes: &[u8]) -> system2::Result<Image> {
    Ok(Image::new(bytes.to_vec())?)
}

fn cached_client(path: &Path) -> system2::Result<Client> {
    Ok(Client::new().cache(path)?)
}

async fn ask_with_no_state(server: &MockServer) -> system2::Result<NoulAnswer> {
    Ok(client_for(server).ask(Noul::new(INSTRUCTIONS)?, "").await?)
}

fn answer(probability: f64) -> system2::Result<NoulAnswer> {
    Ok(Noul::new(INSTRUCTIONS)?.answer(probability)?)
}

#[test]
fn question_error_takes_the_error_of_each_constructor() {
    let noul = questions("", OPTIONS, LEVELS, REGION).unwrap_err();
    let choice = questions(INSTRUCTIONS, &["spam", "spam"], LEVELS, REGION).unwrap_err();
    let score = questions(INSTRUCTIONS, OPTIONS, &[], REGION).unwrap_err();
    let bbox = questions(INSTRUCTIONS, OPTIONS, LEVELS, "").unwrap_err();

    assert!(matches!(
        noul,
        QuestionError::Noul(InvalidNoul::Instructions(InvalidJson::Empty))
    ));
    assert!(matches!(
        choice,
        QuestionError::Choice(InvalidChoice::Duplicate { .. })
    ));
    assert!(matches!(
        score,
        QuestionError::Score(InvalidScore::Count(0))
    ));
    assert!(matches!(
        bbox,
        QuestionError::Bbox(InvalidBbox::Instructions(InvalidJson::Empty))
    ));
    assert_eq!(noul.to_string(), "the instructions must not be empty");
    assert_eq!(
        choice.to_string(),
        "each option must have a different name, but two options have the name `spam`"
    );
    assert_eq!(score.to_string(), "a Score must have 1 to 10 levels, not 0");
    assert_eq!(bbox.to_string(), "the instructions must not be empty");
}

#[tokio::test]
async fn error_takes_each_error_that_a_function_returns() {
    let server = mock_api(noul_response(0.92)).await;

    let noul = questions_or_error("", OPTIONS, LEVELS, REGION).unwrap_err();
    let choice = questions_or_error(INSTRUCTIONS, &["spam", "spam"], LEVELS, REGION).unwrap_err();
    let score = questions_or_error(INSTRUCTIONS, OPTIONS, &[], REGION).unwrap_err();
    let bbox = questions_or_error(INSTRUCTIONS, OPTIONS, LEVELS, "").unwrap_err();
    let question = questions_of_a_set().unwrap_err();
    let answer = answer(1.2).unwrap_err();
    let image = image(include_bytes!("images/long.png")).unwrap_err();
    let ask = ask_with_no_state(&server).await.unwrap_err();
    let missing_folder = cache_path("error").with_extension("");
    let cache = cached_client(&missing_folder.join("cache.jsonl")).unwrap_err();

    assert!(matches!(
        noul,
        Error::Question(QuestionError::Noul(InvalidNoul::Instructions(_)))
    ));
    assert!(matches!(
        choice,
        Error::Question(QuestionError::Choice(InvalidChoice::Duplicate { .. }))
    ));
    assert!(matches!(
        score,
        Error::Question(QuestionError::Score(InvalidScore::Count(0)))
    ));
    assert!(matches!(
        bbox,
        Error::Question(QuestionError::Bbox(InvalidBbox::Instructions(_)))
    ));
    assert!(matches!(
        question,
        Error::Question(QuestionError::Noul(InvalidNoul::Instructions(_)))
    ));
    assert!(matches!(answer, Error::Answer(_)));
    assert!(matches!(image, Error::Image(InvalidImage::Size { .. })));
    assert!(matches!(
        ask,
        Error::Ask(AskError::State(InvalidJson::Empty))
    ));
    assert!(matches!(cache, Error::Cache(CacheError::Read { .. })));
    assert_eq!(noul.to_string(), "the instructions must not be empty");
    assert_eq!(
        answer.to_string(),
        "the answer must give a probability from 0 to 1, not 1.2"
    );
    assert_eq!(ask.to_string(), "the state must not be empty");
}
