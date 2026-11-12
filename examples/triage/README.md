# triage

Triages five support tickets. One request asks three questions about each ticket: is the
customer angry, which team should handle it, and how urgent it is. Plain code then makes the
decision: ask a person when Jev is unsure of the team, page the team's lead when the ticket is
urgent or the customer is angry, and add the ticket to the team's queue otherwise.

## Run

Set `TYPESAFE_API_KEY`, then run:

```sh
cargo run -p triage
```

To replay it with no key, see [the examples](../README.md).

## Output

```text
ticket 1: page the Billing lead
ticket 2: add to the Support queue
ticket 3: page the Engineering lead
ticket 4: add to the Support queue
ticket 5: ask a person
```

Ticket 5 could be billing or engineering. Jev gives engineering 0.80, under the 0.9 that
`TEAM_THRESHOLD` asks for, so a person gets it. Set `TEAM_THRESHOLD` to 0.8 and replay, and the
engineering lead gets it, with no new request.
