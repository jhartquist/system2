mod support;

use support::{INSTRUCTIONS, client_for, mock_api, noul_response};
use system2::{
    AskError, Choice, Error, InvalidChoice, InvalidJson, InvalidNoul, InvalidScore, Noul,
    NoulAnswer, QuestionError, Score,
};
use wiremock::MockServer;

const OPTIONS: &[&str] = &["spam", "ham"];
const LEVELS: &[&str] = &["low", "high"];

type Questions = (Noul, Choice<&'static str>, Score<&'static str>);

// a caller that builds each type of question from data
fn questions(
    rule: &str,
    options: &[&'static str],
    levels: &[&'static str],
) -> Result<Questions, QuestionError> {
    Ok((
        Noul::new(rule)?,
        Choice::new(INSTRUCTIONS, options.to_vec())?,
        Score::new(INSTRUCTIONS, levels.to_vec())?,
    ))
}

// the same caller, with the error of the crate
fn questions_or_error(
    rule: &str,
    options: &[&'static str],
    levels: &[&'static str],
) -> system2::Result<Questions> {
    Ok((
        Noul::new(rule)?,
        Choice::new(INSTRUCTIONS, options.to_vec())?,
        Score::new(INSTRUCTIONS, levels.to_vec())?,
    ))
}

fn questions_of_a_set() -> system2::Result<Questions> {
    Ok(questions("", OPTIONS, LEVELS)?)
}

async fn ask_with_no_state(server: &MockServer) -> system2::Result<NoulAnswer> {
    Ok(client_for(server).ask(Noul::new(INSTRUCTIONS)?, "").await?)
}

#[test]
fn question_error_takes_the_error_of_each_constructor() {
    let noul = questions("", OPTIONS, LEVELS).unwrap_err();
    let choice = questions(INSTRUCTIONS, &["spam", "spam"], LEVELS).unwrap_err();
    let score = questions(INSTRUCTIONS, OPTIONS, &[]).unwrap_err();

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
    assert_eq!(noul.to_string(), "the instructions must not be empty");
    assert_eq!(
        choice.to_string(),
        "each option must have a different name, but two options have the name `spam`"
    );
    assert_eq!(score.to_string(), "a Score must have 1 to 10 levels, not 0");
}

#[tokio::test]
async fn error_takes_each_error_that_a_function_returns() {
    let server = mock_api(noul_response(0.92)).await;

    let noul = questions_or_error("", OPTIONS, LEVELS).unwrap_err();
    let choice = questions_or_error(INSTRUCTIONS, &["spam", "spam"], LEVELS).unwrap_err();
    let score = questions_or_error(INSTRUCTIONS, OPTIONS, &[]).unwrap_err();
    let question = questions_of_a_set().unwrap_err();
    let ask = ask_with_no_state(&server).await.unwrap_err();

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
        question,
        Error::Question(QuestionError::Noul(InvalidNoul::Instructions(_)))
    ));
    assert!(matches!(
        ask,
        Error::Ask(AskError::State(InvalidJson::Empty))
    ));
    assert_eq!(noul.to_string(), "the instructions must not be empty");
    assert_eq!(ask.to_string(), "the state must not be empty");
}
