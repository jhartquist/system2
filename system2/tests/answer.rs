//! Tests of `answer`, which builds the answer of each question type for a test.

mod support;

use serde_json::json;
use support::{STATE, answer_set_response, bbox_answer, client_for, mock_api, noul_answer};
use system2::prelude::*;
use system2::{Bbox, Choice, ChoiceAnswer, Noul, NoulAnswer, Score, ScoreAnswer};

#[derive(Debug, Clone, Copy, PartialEq, Options)]
enum Team {
    Billing,
    Engineering,
    Support,
}

#[derive(Debug, PartialEq, Levels)]
enum Urgency {
    /// The ticket can wait a week.
    Low,
    /// The ticket needs an answer today.
    Medium,
    /// The customer cannot work.
    High,
}

const URGENT: Noul = noul!("Is the ticket urgent?");
const TEAM: Choice<Team> = choice!("Which team must answer the ticket?", Team);
const URGENCY: Score<Urgency> = score!("How urgent is the ticket?", Urgency);
const DIALOG: Bbox = bbox!("The error dialog.");

#[derive(Debug, Answers)]
struct Triage {
    #[system2(question = URGENT)]
    urgent: NoulAnswer,
    #[system2(question = TEAM)]
    team: ChoiceAnswer<Team>,
    #[system2(question = URGENCY)]
    urgency: ScoreAnswer<Urgency>,
}

#[derive(Debug, PartialEq)]
enum Route {
    Page(Team),
    Queue(Team),
    Person,
}

fn route(triage: &Triage) -> Route {
    let team = *triage.team.most_likely();
    if triage.team.confidence() < 0.8 {
        Route::Person
    } else if triage.urgent.probability() >= 0.8 && triage.urgency.score() >= 1.5 {
        Route::Page(team)
    } else {
        Route::Queue(team)
    }
}

#[tokio::test]
async fn answer_gives_the_answer_that_ask_reads() {
    let answers = [
        &noul_answer(0.79),
        &json!({
            "type": "choice",
            "choice": "engineering",
            "confidence": 0.55,
            "probabilities": { "billing": 0.2, "engineering": 0.7, "support": 0.1 },
        }),
        &json!({
            "type": "score",
            "score": 1.6,
            "confidence": 0.7,
            "probabilities": { "0": 0.05, "1": 0.3, "2": 0.65 },
        }),
        &bbox_answer(0.25, 0.1, 0.75, 0.4, 0.9),
    ];
    let server = mock_api(answer_set_response(&answers)).await;
    let read = client_for(&server)
        .ask((URGENT, TEAM, URGENCY, DIALOG), STATE)
        .await
        .unwrap();

    let given = (
        URGENT.answer(0.79).unwrap(),
        TEAM.answer(&Team::Engineering, 0.55, [0.2, 0.7, 0.1])
            .unwrap(),
        URGENCY.answer(1.6, 0.7, [0.05, 0.3, 0.65]).unwrap(),
        DIALOG.answer(0.25, 0.1, 0.75, 0.4, 0.9).unwrap(),
    );

    assert_eq!(format!("{given:?}"), format!("{read:?}"));
    assert_eq!(json!(given), json!(read));
}

#[test]
fn answer_fills_the_struct_of_a_named_set() {
    let triage = |urgent, team_confidence| Triage {
        urgent: URGENT.answer(urgent).unwrap(),
        team: TEAM
            .answer(&Team::Billing, team_confidence, [0.86, 0.10, 0.04])
            .unwrap(),
        urgency: URGENCY.answer(1.6, 0.7, [0.05, 0.30, 0.65]).unwrap(),
    };

    assert_eq!(route(&triage(0.9, 0.79)), Route::Person);
    assert_eq!(route(&triage(0.9, 0.81)), Route::Page(Team::Billing));
    assert_eq!(route(&triage(0.79, 0.9)), Route::Queue(Team::Billing));
    assert_eq!(route(&triage(0.81, 0.9)), Route::Page(Team::Billing));
}

#[test]
fn answer_refuses_a_value_that_the_api_cannot_give() {
    let two_teams = || {
        choice!(
            "Which team?",
            [("billing", Team::Billing), ("support", Team::Support)]
        )
    };
    let cases = [
        (
            URGENT.answer(1.2).unwrap_err(),
            "the answer must give a probability from 0 to 1, not 1.2",
        ),
        (
            URGENT.answer(f64::NAN).unwrap_err(),
            "the answer must give a probability from 0 to 1, not NaN",
        ),
        (
            two_teams()
                .answer(&Team::Engineering, 0.9, [0.5, 0.5])
                .unwrap_err(),
            "the answer must choose an option of the Choice",
        ),
        (
            TEAM.answer(&Team::Billing, 1.1, [0.9, 0.1, 0.0])
                .unwrap_err(),
            "the answer must give a confidence from 0 to 1, not 1.1",
        ),
        (
            TEAM.answer(&Team::Billing, 0.9, [0.9, 0.1]).unwrap_err(),
            "the answer must give a probability for each option of the Choice, and no other",
        ),
        (
            TEAM.answer(&Team::Billing, 0.9, [0.9, 0.1, -0.5])
                .unwrap_err(),
            "the answer must give a probability from 0 to 1, not -0.5",
        ),
        (
            URGENCY.answer(2.5, 0.7, [0.0, 0.0, 1.0]).unwrap_err(),
            "the answer must give a score from 0 to 2, not 2.5",
        ),
        (
            URGENCY.answer(1.6, 0.7, [0.3, 0.7]).unwrap_err(),
            "the answer must give a probability for each level of the Score, and no other",
        ),
        (
            DIALOG.answer(0.25, 0.1, 1.5, 0.4, 0.9).unwrap_err(),
            "the answer must give a right edge from 0 to 1, not 1.5",
        ),
        (
            DIALOG.answer(0.8, 0.1, 0.2, 0.4, 0.9).unwrap_err(),
            "the answer must give a left edge at most its right edge, not 0.8 and 0.2",
        ),
    ];

    for (error, message) in cases {
        assert_eq!(error.to_string(), message);
    }
}
