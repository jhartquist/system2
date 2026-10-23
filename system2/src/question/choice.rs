use crate::ask::{self, private::Sealed};
use crate::{Ask, InvalidJson, json};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use serde_json::value::RawValue;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;

const MAX_OPTIONS: usize = 255;

#[derive(Debug, Clone)]
pub struct Choice<T> {
    instructions: Box<RawValue>,
    options: Vec<Entry<T>>,
}

#[derive(Debug, Clone)]
struct Entry<T> {
    name: String,
    description: Option<Box<RawValue>>,
    value: T,
}

pub trait Options: Sized + 'static {
    // only the derive writes the table, so its shape can change
    #[doc(hidden)]
    const OPTIONS: &'static [ChoiceOption<Self>];
}

#[doc(hidden)]
#[derive(Debug)]
pub struct ChoiceOption<T> {
    pub name: &'static str,
    pub description: Option<&'static str>,
    pub value: fn() -> T,
}

pub trait Label: label::Parts {}

impl<L: label::Parts> Label for L {}

mod label {
    use crate::InvalidJson;
    use serde::Serialize;
    use serde_json::value::RawValue;
    use std::fmt::Display;

    pub trait Name: Display {}

    pub trait Parts {
        fn name(&self) -> String;

        fn description(&self) -> Result<Option<Box<RawValue>>, InvalidJson>;
    }

    impl<N: Name> Parts for N {
        fn name(&self) -> String {
            self.to_string()
        }

        fn description(&self) -> Result<Option<Box<RawValue>>, InvalidJson> {
            Ok(None)
        }
    }

    impl<N: Name, D: Serialize> Parts for (N, D) {
        fn name(&self) -> String {
            self.0.to_string()
        }

        fn description(&self) -> Result<Option<Box<RawValue>>, InvalidJson> {
            crate::json::validate_optional(&self.1)
        }
    }

    impl Name for str {}
    impl Name for String {}
    impl<N: Name + ?Sized> Name for &N {}

    macro_rules! integers {
        ($($integer:ty),*) => { $(impl Name for $integer {})* };
    }

    integers!(
        i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
    );
}

impl<T> Choice<T> {
    pub fn new(
        instructions: impl Serialize,
        items: impl IntoIterator<Item = T>,
    ) -> Result<Self, InvalidChoice>
    where
        T: Display,
    {
        Self::with_values(instructions, items, ToString::to_string)
    }

    pub fn with_descriptions<D: Serialize>(
        instructions: impl Serialize,
        options: impl IntoIterator<Item = (T, D)>,
    ) -> Result<Self, InvalidChoice>
    where
        T: Display,
    {
        let instructions = json::validate(instructions).map_err(InvalidChoice::Instructions)?;
        let options = options
            .into_iter()
            .map(|(value, description)| {
                let name = value.to_string();
                let description = json::validate_optional(description);
                Entry::new(name, description, value)
            })
            .collect::<Result<_, _>>()?;
        Self::checked(instructions, options)
    }

    pub fn with_values<L: Label>(
        instructions: impl Serialize,
        values: impl IntoIterator<Item = T>,
        mut label: impl FnMut(&T) -> L,
    ) -> Result<Self, InvalidChoice> {
        let instructions = json::validate(instructions).map_err(InvalidChoice::Instructions)?;
        let options = values
            .into_iter()
            .map(|value| {
                let label = label(&value);
                Entry::new(label.name(), label.description(), value)
            })
            .collect::<Result<_, _>>()?;
        Self::checked(instructions, options)
    }

    pub fn of(instructions: impl Serialize) -> Result<Self, InvalidChoice>
    where
        T: Options,
    {
        let instructions = json::validate(instructions).map_err(InvalidChoice::Instructions)?;
        let options = T::OPTIONS
            .iter()
            .map(|option| {
                let description = json::validate_optional(option.description);
                Entry::new(option.name.to_owned(), description, (option.value)())
            })
            .collect::<Result<_, _>>()?;
        Self::checked(instructions, options)
    }

    fn checked(instructions: Box<RawValue>, options: Vec<Entry<T>>) -> Result<Self, InvalidChoice> {
        if !(1..=MAX_OPTIONS).contains(&options.len()) {
            return Err(InvalidChoice::Count(options.len()));
        }
        if let Some(index) = options.iter().position(|o| o.name.trim().is_empty()) {
            return Err(InvalidChoice::EmptyName { index });
        }
        let mut names = HashSet::new();
        if let Some(option) = options.iter().find(|o| !names.insert(&o.name)) {
            let name = option.name.clone();
            return Err(InvalidChoice::Duplicate { name });
        }
        Ok(Choice {
            instructions,
            options,
        })
    }
}

impl<T> Entry<T> {
    fn new(
        name: String,
        description: Result<Option<Box<RawValue>>, InvalidJson>,
        value: T,
    ) -> Result<Self, InvalidChoice> {
        let description = match description {
            Ok(description) => description,
            Err(error) => return Err(InvalidChoice::Description { name, error }),
        };
        Ok(Entry {
            name,
            description,
            value,
        })
    }
}

impl<T> Ask for Choice<T> {
    type Answer = ChoiceAnswer<T>;
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidChoice {
    #[error("the instructions {0}")]
    Instructions(InvalidJson),
    #[error("the option at index {index} must have a name that is not empty")]
    EmptyName { index: usize },
    #[error("the description of the option `{name}` {error}")]
    Description { name: String, error: InvalidJson },
    #[error("a Choice must have 1 to 255 options, not {0}")]
    Count(usize),
    #[error("each option must have a different name, but two options have the name `{name}`")]
    Duplicate { name: String },
}

#[derive(Debug, Clone)]
pub struct ChoiceAnswer<T> {
    question: Choice<T>,
    choice: usize,
    confidence: f64,
    probabilities: Vec<f64>,
}

#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct ChoiceProbability<'a, T> {
    pub name: &'a str,
    pub description: Option<&'a RawValue>,
    pub value: &'a T,
    pub probability: f64,
}

impl<T> ChoiceAnswer<T> {
    #[must_use]
    pub fn question(&self) -> &Choice<T> {
        &self.question
    }

    #[must_use]
    pub fn into_question(self) -> Choice<T> {
        self.question
    }

    #[must_use]
    pub fn most_likely(&self) -> &T {
        &self.question.options[self.choice].value
    }

    #[must_use]
    pub fn into_most_likely(self) -> T {
        let mut options = self.question.options;
        options.swap_remove(self.choice).value
    }

    pub fn top(&self, n: usize) -> impl Iterator<Item = (&T, f64)> {
        let mut ranked: Vec<_> = self.probabilities().collect();

        // a stable sort keeps equal options in the order of the question
        ranked.sort_by(|a, b| b.probability.total_cmp(&a.probability));
        ranked.into_iter().take(n).map(|o| (o.value, o.probability))
    }

    #[must_use]
    pub fn confidence(&self) -> f64 {
        self.confidence
    }

    #[must_use]
    pub fn probabilities(&self) -> impl ExactSizeIterator<Item = ChoiceProbability<'_, T>> {
        self.question
            .options
            .iter()
            .zip(&self.probabilities)
            .map(|(option, &probability)| ChoiceProbability {
                name: &option.name,
                description: option.description.as_deref(),
                value: &option.value,
                probability,
            })
    }

    #[must_use]
    pub fn into_probabilities(self) -> Vec<(T, f64)> {
        self.question
            .options
            .into_iter()
            .zip(self.probabilities)
            .map(|(option, probability)| (option.value, probability))
            .collect()
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename = "choice")]
struct Question<'a, T> {
    instructions: &'a RawValue,
    #[serde(serialize_with = "criteria")]
    criteria: &'a [Entry<T>],
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Answer {
    Choice {
        choice: String,
        confidence: f64,
        probabilities: HashMap<String, f64>,
    },
}

impl<T> Sealed for Choice<T> {
    fn to_json(&self) -> impl Serialize {
        let instructions = &self.instructions;
        let criteria = self.options.as_slice();
        Question {
            instructions,
            criteria,
        }
    }

    fn into_answer(self, json: Value) -> Result<<Self as Ask>::Answer, String> {
        let Answer::Choice {
            choice,
            confidence,
            mut probabilities,
        } = ask::from_json(json)?;
        let choice = self
            .options
            .iter()
            .position(|o| o.name == choice)
            .ok_or_else(|| format!("must choose an option of the Choice, not `{choice}`"))?;
        let count = probabilities.len();
        let probabilities = self
            .options
            .iter()
            .map(|o| probabilities.remove(&o.name))
            .collect::<Option<Vec<_>>>()
            .filter(|_| count == self.options.len())
            .ok_or("must give a probability for each option of the Choice, and no other")?;
        let question = self;
        Ok(ChoiceAnswer {
            question,
            choice,
            confidence,
            probabilities,
        })
    }
}

// the options keep their order, which a json map sorts
fn criteria<T, S: Serializer>(options: &&[Entry<T>], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_map(options.iter().map(|o| (&o.name, &o.description)))
}

#[cfg(test)]
mod tests {
    use super::{Choice, ChoiceAnswer, ChoiceOption, InvalidChoice, Options};
    use crate::InvalidJson;
    use crate::ask::private::Sealed;
    use serde_json::value::RawValue;
    use serde_json::{Map, Value, json};
    use std::collections::BTreeMap;

    fn serialized<T>(choice: &Choice<T>) -> String {
        serde_json::to_string(&choice.to_json()).unwrap()
    }

    fn answer(choice: &str, probabilities: &Value) -> Value {
        json!({
            "type": "choice",
            "choice": choice,
            "confidence": 0.6,
            "probabilities": probabilities,
        })
    }

    // one type of option, so coverage sees each check of `checked` and its success together
    fn numbered(count: usize) -> Vec<(String, Value)> {
        (0..count).map(|i| (format!("o{i}"), json!(null))).collect()
    }

    #[derive(Debug)]
    struct Problem;

    impl Options for Problem {
        const OPTIONS: &'static [ChoiceOption<Self>] = &[
            ChoiceOption {
                name: "crash",
                description: Some("The code panics for some input."),
                value: || Problem,
            },
            ChoiceOption {
                name: "no_problem",
                description: None,
                value: || Problem,
            },
        ];
    }

    fn languages() -> ChoiceAnswer<&'static str> {
        let choice = Choice::new("Which language?", ["rust", "python", "go"]).unwrap();
        let probabilities = json!({ "rust": 0.2, "python": 0.4, "go": 0.4 });
        choice.into_answer(answer("go", &probabilities)).unwrap()
    }

    #[test]
    fn new_names_each_item_by_its_display() {
        let years = Choice::new("In which year?", 2026..=2027).unwrap();
        let languages = Choice::new("Which language?", ["rust", "python", "go"]).unwrap();

        assert_eq!(
            serialized(&years),
            r#"{"type":"choice","instructions":"In which year?","criteria":{"2026":null,"2027":null}}"#
        );
        assert_eq!(
            serialized(&languages),
            r#"{"type":"choice","instructions":"Which language?","criteria":{"rust":null,"python":null,"go":null}}"#
        );
    }

    #[test]
    fn with_descriptions_takes_pairs_or_a_map() {
        let pairs = vec![("block", "Ask for a fix."), ("approve", "Approve it.")];
        let sorted = BTreeMap::from([
            ("block", json!(null)),
            ("approve", json!({ "when": "tests pass" })),
        ]);
        let map: Map<String, Value> =
            Map::from_iter([("block".to_owned(), json!("Ask for a fix."))]);

        let cases = [
            (
                serialized(&Choice::with_descriptions("What next?", pairs).unwrap()),
                r#"{"block":"Ask for a fix.","approve":"Approve it."}"#,
            ),
            (
                serialized(&Choice::with_descriptions("What next?", &sorted).unwrap()),
                r#"{"approve":{"when":"tests pass"},"block":null}"#,
            ),
            (
                serialized(&Choice::with_descriptions("What next?", &map).unwrap()),
                r#"{"block":"Ask for a fix."}"#,
            ),
        ];
        for (text, criteria) in cases {
            assert_eq!(
                text,
                format!(r#"{{"type":"choice","instructions":"What next?","criteria":{criteria}}}"#)
            );
        }
    }

    #[test]
    fn with_values_takes_a_name_or_a_name_and_a_description() {
        struct Skill {
            id: u32,
            name: String,
            description: Option<&'static str>,
        }
        let skills = [
            Skill {
                id: 7,
                name: "search".to_owned(),
                description: Some("Find a file."),
            },
            Skill {
                id: 3,
                name: "edit".to_owned(),
                description: None,
            },
        ];

        let by_id = Choice::with_values("Which skill?", &skills, |s| s.id).unwrap();
        let by_name = Choice::with_values("Which skill?", &skills, |s| &s.name).unwrap();
        let described = Choice::with_values("Which skill?", &skills, |s| {
            (s.name.as_str(), s.description)
        })
        .unwrap();

        let criteria = |choice| {
            serialized(choice)
                .split_once(r#""criteria":"#)
                .unwrap()
                .1
                .to_owned()
        };
        assert_eq!(criteria(&by_id), r#"{"7":null,"3":null}}"#);
        assert_eq!(criteria(&by_name), r#"{"search":null,"edit":null}}"#);
        assert_eq!(
            criteria(&described),
            r#"{"search":"Find a file.","edit":null}}"#
        );
    }

    #[test]
    fn of_takes_the_options_of_the_type() {
        let choice = Choice::<Problem>::of("What is the worst problem?").unwrap();

        assert_eq!(
            serialized(&choice),
            concat!(
                r#"{"type":"choice","instructions":"What is the worst problem?","#,
                r#""criteria":{"crash":"The code panics for some input.","no_problem":null}}"#,
            )
        );
    }

    #[test]
    fn of_refuses_a_bad_table() {
        #[derive(Debug)]
        struct Twice;
        impl Options for Twice {
            const OPTIONS: &'static [ChoiceOption<Self>] = &[
                ChoiceOption {
                    name: "crash",
                    description: None,
                    value: || Twice,
                },
                ChoiceOption {
                    name: "crash",
                    description: None,
                    value: || Twice,
                },
            ];
        }
        #[derive(Debug)]
        struct Blank;
        impl Options for Blank {
            const OPTIONS: &'static [ChoiceOption<Self>] = &[ChoiceOption {
                name: "crash",
                description: Some(" "),
                value: || Blank,
            }];
        }

        let cases = [
            (
                Choice::<Twice>::of("Which one?").unwrap_err(),
                "each option must have a different name, but two options have the name `crash`",
            ),
            (
                Choice::<Blank>::of("Which one?").unwrap_err(),
                "the description of the option `crash` must not be empty",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn with_descriptions_refuses_bad_options() {
        let options = |names: &[&str]| -> Vec<(String, Value)> {
            names
                .iter()
                .map(|name| ((*name).to_owned(), json!(null)))
                .collect()
        };
        let cases = [
            (options(&[]), "a Choice must have 1 to 255 options, not 0"),
            (
                numbered(256),
                "a Choice must have 1 to 255 options, not 256",
            ),
            (
                options(&["rust", ""]),
                "the option at index 1 must have a name that is not empty",
            ),
            (
                options(&["rust", " \n"]),
                "the option at index 1 must have a name that is not empty",
            ),
            (
                vec![("rust".to_owned(), json!(""))],
                "the description of the option `rust` must not be empty",
            ),
            (
                vec![("rust".to_owned(), json!(1))],
                "the description of the option `rust` must be text, an object, or an array",
            ),
            (
                options(&["go", "rust", "go"]),
                "each option must have a different name, but two options have the name `go`",
            ),
        ];
        for (options, message) in cases {
            let error = Choice::with_descriptions("Which language?", options).unwrap_err();
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn with_descriptions_takes_1_to_255_options() {
        for count in [1, 255] {
            let choice = Choice::with_descriptions("Which one?", numbered(count));
            assert!(choice.is_ok(), "{count}");
        }
    }

    #[test]
    fn constructors_refuse_bad_instructions() {
        let errors = [
            Choice::new("", ["rust"]).unwrap_err(),
            Choice::with_descriptions(" ", [("rust", ())]).unwrap_err(),
            Choice::with_values(json!({}), ["rust"], |name| *name).unwrap_err(),
            Choice::<Problem>::of(json!([])).unwrap_err(),
        ];
        for error in errors {
            assert!(matches!(
                error,
                InvalidChoice::Instructions(InvalidJson::Empty)
            ));
            assert_eq!(error.to_string(), "the instructions must not be empty");
        }
    }

    #[test]
    fn into_answer_keeps_the_values_of_the_api() {
        let answer = languages();

        assert_eq!(*answer.most_likely(), "go");
        assert_eq!(answer.confidence(), 0.6);
        let probabilities: Vec<_> = answer
            .probabilities()
            .map(|option| (option.name, *option.value, option.probability))
            .collect();
        assert_eq!(
            probabilities,
            [
                ("rust", "rust", 0.2),
                ("python", "python", 0.4),
                ("go", "go", 0.4)
            ]
        );
    }

    #[test]
    fn into_answer_refuses_an_answer_that_does_not_fit() {
        let cases = [
            (
                json!({ "type": "noul", "noul": 0.5 }),
                "must answer each question: unknown variant `noul`, expected `choice`",
            ),
            (
                answer("java", &json!({ "rust": 0.5, "go": 0.5 })),
                "must choose an option of the Choice, not `java`",
            ),
            (
                answer("go", &json!({ "go": 1.0 })),
                "must give a probability for each option of the Choice, and no other",
            ),
            (
                answer("go", &json!({ "rust": 0.5, "java": 0.5 })),
                "must give a probability for each option of the Choice, and no other",
            ),
            (
                answer("go", &json!({ "rust": 0.4, "go": 0.4, "java": 0.2 })),
                "must give a probability for each option of the Choice, and no other",
            ),
        ];
        for (json, message) in cases {
            let choice = Choice::new("Which language?", ["rust", "go"]).unwrap();
            assert_eq!(choice.into_answer(json).unwrap_err(), message);
        }
    }

    #[test]
    fn probabilities_has_the_descriptions() {
        let choice = Choice::with_descriptions(
            "What next?",
            [("block", Some("Ask for a fix.")), ("approve", None)],
        )
        .unwrap();
        let answer = choice
            .into_answer(answer("block", &json!({ "block": 0.9, "approve": 0.1 })))
            .unwrap();

        let descriptions: Vec<_> = answer
            .probabilities()
            .map(|option| option.description.map(RawValue::get))
            .collect();
        assert_eq!(descriptions, [Some(r#""Ask for a fix.""#), None]);
        assert_eq!(answer.probabilities().len(), 2);
    }

    #[test]
    fn top_ranks_by_probability_then_by_order() {
        let answer = languages();

        let top: Vec<_> = answer.top(2).collect();
        let all: Vec<_> = answer.top(5).collect();

        assert_eq!(top, [(&"python", 0.4), (&"go", 0.4)]);
        assert_eq!(all, [(&"python", 0.4), (&"go", 0.4), (&"rust", 0.2)]);
    }

    #[test]
    fn into_methods_give_the_values_back() {
        assert_eq!(languages().into_most_likely(), "go");
        assert_eq!(
            languages().into_probabilities(),
            [("rust", 0.2), ("python", 0.4), ("go", 0.4)]
        );

        let question = languages().into_question();
        assert_eq!(serialized(languages().question()), serialized(&question));
        assert!(serialized(&question).contains(r#""instructions":"Which language?""#));
    }
}
