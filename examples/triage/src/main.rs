//! Triages five support tickets. One request asks three questions about each ticket, and plain
//! code makes the decision from the answers.

use serde::Serialize;
use system2::prelude::*;

// the probability of its team that a ticket needs before the code acts on it
const TEAM_THRESHOLD: f64 = 0.9;

#[derive(Serialize)]
struct Ticket {
    id: u64,
    plan: &'static str,
    message: &'static str,
}

#[derive(Debug, Clone, Copy, Options)]
enum Team {
    /// Payments, refunds, and invoices.
    Billing,
    /// Bugs and outages.
    Engineering,
    /// Anything else.
    Support,
}

#[derive(Debug, Levels)]
enum Urgency {
    Low,
    Medium,
    High,
}

const TICKETS: [Ticket; 5] = [
    Ticket {
        id: 1,
        plan: "enterprise",
        message: "We were charged twice this month. Please fix it today.",
    },
    Ticket {
        id: 2,
        plan: "free",
        message: "How do I change my avatar?",
    },
    Ticket {
        id: 3,
        plan: "pro",
        message: "The export page has shown a 500 error since this morning. \
                  Our whole team is blocked.",
    },
    Ticket {
        id: 4,
        plan: "pro",
        message: "hello??",
    },
    Ticket {
        id: 5,
        plan: "pro",
        message: "The invoice page crashes when I try to pay.",
    },
];

#[tokio::main]
async fn main() -> system2::Result<()> {
    for ticket in &TICKETS {
        let triage = questions! {
            angry: noul!("Is the customer angry?"),
            team: choice!("Which team should handle this ticket?", Team),
            urgency: score!("How urgent is this ticket?", Urgency),
        }
        .ask(ticket)
        .await?;

        #[expect(clippy::expect_used, reason = "a Choice has at least one option")]
        let (&team, team_probability) = triage.team.top(1).next().expect("a Choice has an option");
        let urgent = triage.urgency.score() >= 1.5;
        let angry = triage.angry.probability() > 0.8;

        let decision = if team_probability < TEAM_THRESHOLD {
            "ask a person".to_string()
        } else if urgent || angry {
            format!("page the {team:?} lead")
        } else {
            format!("add to the {team:?} queue")
        };
        println!("ticket {}: {decision}", ticket.id);
    }
    Ok(())
}
