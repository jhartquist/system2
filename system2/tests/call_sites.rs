mod support;

use serde::Serialize;
use serde_json::json;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use support::{AnswerByType, client_for, mount};
use system2::prelude::*;
use system2::{Choice, Noul, NoulAnswer, QuestionError, Score, Tagged};
use tokio::task::JoinSet;
use wiremock::MockServer;

const CODE: &str = "fn add(a: i32, b: i32) -> i32 { a - b }";

// the global client is one for each process, and its connections stay on the runtime that
// opened them, so the call sites run in turn in one test
#[tokio::test]
async fn ask_runs_each_call_site_as_written() -> system2::Result<()> {
    let server = MockServer::start().await;
    mount(&server, AnswerByType).await;
    client_for(&server).set_global();

    one_question(CODE).await?;
    questions_the_compiler_checks(CODE).await?;
    choice(CODE).await?;
    score(CODE).await?;
    questions_from_data(CODE).await?;
    sets(CODE).await?;
    sets_with_names(CODE).await?;
    states(CODE).await?;
    an_answer_and_its_question(CODE).await?;
    decisions(CODE).await
}

async fn one_question(code: &str) -> system2::Result<()> {
    let mut decisions = Vec::new();

    let answer = noul!("Is there a bug in this code?").ask(code).await?;
    if answer.is_likely() {
        decisions.push("likely");
    }
    if answer.probability() > 0.9 {
        decisions.push("sure");
    }

    assert_eq!(decisions, ["likely", "sure"]);
    Ok(())
}

const BUGGY: Noul = noul!("Is there a bug in this code?");

const TESTED: Noul = noul!("Is the function tested?", {
    "true": "A test calls the function.",
    "false": "No test calls the function.",
});

const POLICY: Noul = noul!({
    "question": "Does the change obey the policy?",
    "policy": "Credentials never appear in source code.",
});

const PURE_TEXT: &str = "Is the function pure?";
const PURE: Noul = noul!(PURE_TEXT);

async fn questions_the_compiler_checks(code: &str) -> system2::Result<()> {
    let function = "add";
    let rule_text = String::from("Each function has a test.");

    let tested = noul!("Is `{function}` tested?");
    let obeys = noul!({ "question": "Does the code obey the rule?", "rule": rule_text });

    let answers = (BUGGY, TESTED, POLICY, PURE, tested, obeys)
        .ask(code)
        .await?;
    let (buggy, tested_function, policy, pure, tested, obeys) = answers;
    let probabilities =
        [buggy, tested_function, policy, pure, tested, obeys].map(|answer| answer.probability());
    assert_eq!(probabilities, [0.95; 6]);
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Options)]
enum Problem {
    /// The code returns the wrong value for some input.
    WrongResult,
    /// The code panics for some input.
    Crash,
    /// The code is correct.
    #[system2(rename = "no_problem", description = "The code has no problem.")]
    None,
}

const PROBLEM: Choice<Problem> = choice!("What is the worst problem in this code?", Problem);

const NEXT: Choice<&str> = choice!("What must the reviewer do?", {
    "approve": "Approve the change.",
    "block": "Ask for a fix.",
});

const LANGUAGE: Choice<&str> = choice!("Which language is the code in?", ["rust", "python", "go"]);

const BETTER: Choice<Ordering> = choice!(
    "Which answer is better?",
    [
        ("first", "The first answer is better.", Ordering::Less),
        ("second", "The second answer is better.", Ordering::Greater),
        ("same", "The two answers are equally good.", Ordering::Equal),
    ]
);

async fn choice(code: &str) -> system2::Result<()> {
    let problem = PROBLEM.ask(code).await?;
    let worst = match problem.most_likely() {
        Problem::Crash => "crash",
        _ => "other",
    };
    assert_eq!(worst, "crash");

    let sure = problem.confidence() > 0.8;
    assert!(sure);

    let mut top = Vec::new();
    for (value, probability) in problem.top(2) {
        top.push((*value, probability));
    }
    assert_eq!(top.len(), 2);
    assert_eq!(top[0], (Problem::Crash, 1.0));

    for (value, probability) in problem.probabilities() {
        println!("{value:?} {probability:.2}");
    }
    let owned: Vec<(Problem, f64)> = problem.into_probabilities();
    assert_eq!(owned.len(), 3);

    let (next, language, better) = (NEXT, LANGUAGE, BETTER).ask(code).await?;
    assert_eq!(*next.most_likely(), "approve");
    assert_eq!(*language.most_likely(), "go");
    assert_eq!(*better.most_likely(), Ordering::Less);
    Ok(())
}

/// The levels, from the lowest to the highest. The doc comments are the levels.
#[derive(Debug, Clone, Copy, PartialEq, Levels)]
enum Severity {
    /// Nothing breaks.
    Minor,
    /// A feature breaks for some input.
    Major,
    /// Data is lost or corrupted.
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Levels)]
enum Effort {
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Levels)]
enum Risk {
    /// The default for a new change.
    #[system2(description = "Low: the change touches no data.")]
    Low,
    /// The change needs a second review.
    #[system2(description = "High: the change can lose data.")]
    High,
}

const SEVERITY: Score<Severity> = score!("How severe is the problem?", Severity);
const SIZE: Score<&str> = score!(
    "How big is the change?",
    ["A line.", "A function.", "A module."]
);
const DAYS: Score<u32> = score!(
    "How long does the change take?",
    [("A day.", 1), ("A week.", 5)]
);

async fn score(code: &str) -> system2::Result<()> {
    let mut decisions = Vec::new();

    let severity = SEVERITY.ask(code).await?;
    let mean = severity.score();
    assert_eq!(mean, 2.0);
    for (level, probability) in severity.probabilities() {
        println!("{level:?} {probability:.2}");
    }
    if *severity.most_likely() == Severity::Critical && severity.confidence() > 0.7 {
        decisions.push("escalate");
    }
    assert_eq!(decisions, ["escalate"]);

    let effort = score!("How much effort does the change take?", Effort);
    let risk = score!("How risky is the change?", Risk);
    let (size, days, effort, risk) = (SIZE, DAYS, effort, risk).ask(code).await?;
    assert_eq!(*size.most_likely(), "A module.");
    assert_eq!(*days.most_likely(), 5);
    assert_eq!(*effort.most_likely(), Effort::Large);
    assert_eq!(*risk.most_likely(), Risk::High);
    Ok(())
}

struct Config {
    rule: String,
    question: String,
}

struct File {
    path: String,
}

struct Skill {
    name: String,
    description: String,
}

struct RiskLevel {
    text: String,
}

async fn questions_from_data(code: &str) -> system2::Result<()> {
    let config = Config {
        rule: String::from("Each function has a test."),
        question: String::from("What is wrong with the code?"),
    };
    let teams = ["platform", "billing"];
    let actions = BTreeMap::from([
        ("block", "Ask for a fix."),
        ("approve", "Approve the change."),
    ]);
    let files = vec![
        File {
            path: String::from("src/main.rs"),
        },
        File {
            path: String::from("src/lib.rs"),
        },
    ];
    let question = String::from("Which skill does the task need?");
    let skills = vec![Skill {
        name: String::from("rust"),
        description: String::from("Writes Rust code."),
    }];
    let sizes = ["A line.", "A function.", "A module."];
    let risks = vec![
        RiskLevel {
            text: String::from("The change touches no data."),
        },
        RiskLevel {
            text: String::from("The change can lose data."),
        },
    ];
    let diff = "- a + b\n+ a - b";

    let rule = Noul::new(&config.rule)?;
    let rule_with_criteria =
        Noul::with_criteria(&config.rule, json!({ "true": "The code obeys the rule." }))?;

    let team = Choice::new("Which team owns this ticket?", &teams)?;
    let year = Choice::new("In which year is the filing?", 2020..=2026)?;

    let next = Choice::with_descriptions("What must the reviewer do?", &actions)?;

    let file: &File = Choice::with_values("Which file needs a review most?", &files, |f| &f.path)?
        .ask(diff)
        .await?
        .into_most_likely();
    assert_eq!(file.path, "src/lib.rs");
    let skill = Choice::with_values(&question, &skills, |s| (&s.name, &s.description))?;

    let problem = Choice::<Problem>::of(&config.question)?;

    let size = Score::new("How big is the change?", &sizes)?;
    let risk = Score::with_values("How risky is the change?", &risks, |r| &r.text)?;
    let severity = Score::<Severity>::of(&config.question)?;

    let question_set = (
        rule,
        rule_with_criteria,
        team,
        year,
        next,
        skill,
        problem,
        size,
        risk,
        severity,
    );
    let (rule, rule_with_criteria, team, year, next, skill, problem, size, risk, severity) =
        question_set.ask(code).await?;
    assert!(rule.is_likely() && rule_with_criteria.is_likely());
    assert_eq!(**team.most_likely(), "billing");
    assert_eq!(*year.most_likely(), 2020);
    assert_eq!(**next.most_likely(), "approve");
    assert_eq!(skill.most_likely().name, "rust");
    assert_eq!(*problem.most_likely(), Problem::Crash);
    assert_eq!(**size.most_likely(), "A module.");
    assert_eq!(risk.most_likely().text, "The change can lose data.");
    assert_eq!(*severity.most_likely(), Severity::Critical);
    Ok(())
}

const SECURE: Noul = noul!("Is the code secure?");

const NAMED: [Tagged<Noul, &str>; 2] = [BUGGY.tag("buggy"), TESTED.tag("tested")];

struct Rule {
    text: String,
}

async fn sets(code: &str) -> system2::Result<()> {
    // no traits at all, so the tag comes back moved
    struct Change {
        id: u32,
    }

    let rules = [
        Rule {
            text: String::from("Each function has a test."),
        },
        Rule {
            text: String::from("No function panics."),
        },
    ];
    let texts = ["spam", "a reply"];
    let is_public = false;
    let change = Change { id: 7 };

    let (buggy, tested) = (BUGGY, TESTED).ask(code).await?;
    assert!(buggy.is_likely() && tested.is_likely());

    let [buggy, tested] = [BUGGY, TESTED].ask(code).await?;
    assert!(buggy.is_likely() && tested.is_likely());
    let answers: Vec<NoulAnswer> = rules
        .iter()
        .map(|r| Noul::new(&r.text))
        .collect::<Result<Vec<_>, _>>()?
        .ask(code)
        .await?;
    assert_eq!(answers.len(), 2);
    let answers = texts.iter().map(|t| noul!("Is it {t}?")).ask(code).await?;
    assert_eq!(answers.len(), 2);

    let (buggy, security) = (BUGGY, is_public.then_some(SECURE)).ask(code).await?;
    assert!(buggy.is_likely() && security.is_none());

    let checks = texts
        .iter()
        .map(|t| noul!("Is it {t}?").tag(t))
        .ask(code)
        .await?;
    let mut checked = Vec::new();
    for (likely, text) in checks {
        checked.push((likely.is_likely(), *text));
    }
    assert_eq!(checked, [(true, "spam"), (true, "a reply")]);
    let (buggy, change) = BUGGY.tag(change).ask(code).await?;
    assert!(buggy.is_likely());
    assert_eq!(change.id, 7);

    let named = NAMED.ask(code).await?;
    assert_eq!(named.map(|(_, name)| name), ["buggy", "tested"]);
    Ok(())
}

#[derive(Debug, Answers)]
pub struct Review {
    #[system2(question = BUGGY)]
    pub buggy: NoulAnswer,
    #[system2(question = noul!("Is the function tested?"))]
    pub tested: NoulAnswer,
}

const REVIEW: Questions<Review> = Questions::new();

impl Review {
    fn is_risky(&self) -> bool {
        self.buggy.is_likely() && !self.tested.is_likely()
    }
}

async fn sets_with_names(code: &str) -> system2::Result<()> {
    struct Change {
        id: u32,
    }

    let change = Change { id: 7 };
    let rules = [Rule {
        text: String::from("Each function has a test."),
    }];
    let teams = ["platform", "billing"];

    let review = questions! {
        buggy: BUGGY,
        problem: PROBLEM,
        severity: score!("How severe is it?", ["Cosmetic.", "A feature breaks.", "Data is lost."]),
        style: questions! { idiomatic: noul!("Is the code idiomatic?") },
    }
    .ask(code)
    .await?;
    let mut decisions = Vec::new();
    if review.buggy.is_likely() && review.style.idiomatic.is_likely() {
        decisions.push("review");
    }
    assert_eq!(decisions, ["review"]);
    assert_eq!(*review.problem.most_likely(), Problem::Crash);
    assert_eq!(*review.severity.most_likely(), "Data is lost.");

    let (review, id) = questions! { buggy: BUGGY, tested: TESTED }
        .tag(change.id)
        .ask(code)
        .await?;
    assert!(review.buggy.is_likely() && review.tested.is_likely());
    assert_eq!(id, 7);

    let review = REVIEW.ask(code).await?;
    assert!(!review.is_risky());
    let review: Review = Questions::new().ask(code).await?;
    assert!(!review.is_risky());

    let checks = rules
        .iter()
        .map(|rule| Ok(Noul::new(&rule.text)?.tag(rule)))
        .collect::<Result<Vec<_>, QuestionError>>()?;
    let review = questions! {
        checks,
        team: Choice::new("Which team owns this code?", &teams)?,
        severity: SEVERITY,
    }
    .ask(code)
    .await?;
    let mut broken_rules = Vec::new();
    for (broken, rule) in review.checks {
        if broken.is_likely() {
            broken_rules.push(rule.text.as_str());
        }
    }
    assert_eq!(broken_rules, ["Each function has a test."]);
    assert_eq!(**review.team.most_likely(), "billing");
    assert_eq!(*review.severity.most_likely(), Severity::Critical);
    Ok(())
}

#[derive(Serialize)]
struct Change {
    diff: String,
    description: String,
}

const DESCRIBED: Noul = noul!("Does the description match the diff?");

async fn described(change: &Change) -> system2::Result<NoulAnswer> {
    Ok(DESCRIBED.ask(change).await?)
}

async fn states(code: &str) -> system2::Result<()> {
    let diff = "- a + b\n+ a - b";
    let tests = ["add_adds"];
    let change = Change {
        diff: String::from(diff),
        description: String::from("Fix the sign in add."),
    };

    BUGGY.ask(code).await?;
    BUGGY.ask(&json!({ "diff": diff, "tests": tests })).await?;
    BUGGY.ask(&change).await?;

    assert!(described(&change).await?.is_likely());
    Ok(())
}

async fn an_answer_and_its_question(code: &str) -> system2::Result<()> {
    let files = vec![
        File {
            path: String::from("src/main.rs"),
        },
        File {
            path: String::from("src/lib.rs"),
        },
    ];
    let diffs = ["- a + b\n+ a - b", "- a * b\n+ a / b"];

    let answer = PROBLEM.ask(code).await?;
    let asked: &Choice<Problem> = answer.question();
    let again = asked.clone().ask(code).await?;
    assert_eq!(*again.most_likely(), Problem::Crash);

    let mut question =
        Choice::with_values("Which file needs a review most?", files, |f| f.path.clone())?;
    for diff in diffs {
        let answer = question.ask(diff).await?;
        question = answer.into_question();
    }
    let file = question.ask(code).await?.into_most_likely();
    assert_eq!(file.path, "src/lib.rs");
    Ok(())
}

const URGENT: Noul = noul!("Is the ticket urgent?");

#[derive(Serialize)]
struct Ticket {
    id: u32,
    text: String,
}

async fn decisions(code: &str) -> system2::Result<()> {
    let tickets = vec![
        Ticket {
            id: 1,
            text: String::from("The site is down."),
        },
        Ticket {
            id: 2,
            text: String::from("Please add a dark theme."),
        },
    ];

    let mut needs_a_test = false;
    if BUGGY.ask(code).await?.probability() >= 0.2 && !TESTED.ask(code).await?.is_likely() {
        needs_a_test = true;
    }
    assert!(!needs_a_test);

    let mut asking = JoinSet::new();
    for ticket in &tickets {
        asking.spawn(URGENT.tag(ticket.id).ask(ticket));
    }
    let mut urgent_ids = Vec::new();
    for checked in asking.join_all().await {
        let (urgent, id) = checked?;
        if urgent.is_likely() {
            urgent_ids.push(id);
        }
    }
    urgent_ids.sort_unstable();
    assert_eq!(urgent_ids, [1, 2]);
    Ok(())
}
