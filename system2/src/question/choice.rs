use super::table::Table;
use crate::ask::wire::{RawAnswerSet, RawQuestionSet, Sealed, validate_range};
use crate::json::Json;
use crate::{InvalidJson, QuestionSet, Tagged, json};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::value::RawValue;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;

const MAX_OPTIONS: usize = 255;

#[derive(Debug, Clone)]
pub struct Choice<O> {
    instructions: Json,
    options: Table<MacroOption<O>, Entry<O>>,
}

// an option as a macro writes it: the name, the description, and the value
type MacroOption<O> = (&'static str, Option<Json>, O);

#[derive(Debug, Clone)]
struct Entry<O> {
    name: Cow<'static, str>,
    description: Option<Json>,
    value: O,
}

#[diagnostic::on_unimplemented(
    message = "`{Self}` must derive `Options` to give the options of a Choice",
    label = "does not derive `Options`",
    note = "add `#[derive(Options)]` to an enum, or use `Choice::with_values` for other types"
)]
pub trait Options: Sized + 'static {
    // only the derive writes the table, so its shape can change
    #[doc(hidden)]
    const OPTIONS: &'static [ChoiceOption<Self>];

    // `choice!` names the type in a path to this, where rustc refuses a variable as not a type
    #[doc(hidden)]
    const MACRO_OPTIONS: fn() -> Vec<MacroOption<Self>> = options_of::<Self>;
}

#[doc(hidden)]
#[derive(Debug)]
pub struct ChoiceOption<O> {
    pub name: &'static str,
    pub description: Option<&'static str>,
    pub value: fn() -> O,
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

impl<O> Choice<O> {
    pub fn new(
        instructions: impl Serialize,
        items: impl IntoIterator<Item = O>,
    ) -> Result<Self, InvalidChoice>
    where
        O: Display,
    {
        Self::with_values(instructions, items, ToString::to_string)
    }

    pub fn with_descriptions<D: Serialize>(
        instructions: impl Serialize,
        options: impl IntoIterator<Item = (O, D)>,
    ) -> Result<Self, InvalidChoice>
    where
        O: Display,
    {
        let instructions = json::validate(instructions).map_err(InvalidChoice::Instructions)?;
        let options = options
            .into_iter()
            .map(|(value, description)| {
                let name = Cow::Owned(value.to_string());
                let description = json::validate_optional(description);
                Entry::new(name, description, value)
            })
            .collect::<Result<_, _>>()?;
        Self::validated(instructions, options)
    }

    pub fn with_values<L: Label>(
        instructions: impl Serialize,
        values: impl IntoIterator<Item = O>,
        mut label: impl FnMut(&O) -> L,
    ) -> Result<Self, InvalidChoice> {
        let instructions = json::validate(instructions).map_err(InvalidChoice::Instructions)?;
        let options = values
            .into_iter()
            .map(|value| {
                let label = label(&value);
                Entry::new(Cow::Owned(label.name()), label.description(), value)
            })
            .collect::<Result<_, _>>()?;
        Self::validated(instructions, options)
    }

    pub fn of(instructions: impl Serialize) -> Result<Self, InvalidChoice>
    where
        O: Options,
    {
        let instructions = json::validate(instructions).map_err(InvalidChoice::Instructions)?;
        let options = O::OPTIONS
            .iter()
            .map(|option| {
                let description = json::validate_optional(option.description);
                Entry::new(Cow::Borrowed(option.name), description, (option.value)())
            })
            .collect::<Result<_, _>>()?;
        Self::validated(instructions, options)
    }

    pub const fn tag<T>(self, tag: T) -> Tagged<Self, T> {
        Tagged::new(self, tag)
    }

    fn validated(
        instructions: Box<RawValue>,
        options: Vec<Entry<O>>,
    ) -> Result<Self, InvalidChoice> {
        if !(1..=MAX_OPTIONS).contains(&options.len()) {
            return Err(InvalidChoice::Count(options.len()));
        }
        if let Some(index) = options.iter().position(|o| o.name.trim().is_empty()) {
            return Err(InvalidChoice::EmptyName { index });
        }
        let mut names = HashSet::new();
        if let Some(option) = options.iter().find(|o| !names.insert(&o.name)) {
            let name = option.name.to_string();
            return Err(InvalidChoice::Duplicate { name });
        }
        let instructions = Json::from(instructions);
        let options = Table::Owned(options);
        Ok(Choice {
            instructions,
            options,
        })
    }
}

#[doc(hidden)]
#[must_use]
pub const fn choice<O>(instructions: Json, options: fn() -> Vec<MacroOption<O>>) -> Choice<O> {
    let options = Table::Deferred(options);
    Choice {
        instructions,
        options,
    }
}

// a description from run time cannot go in a `fn`, so the macro gives the options owned
#[doc(hidden)]
#[must_use]
pub fn owned_choice<O>(instructions: Json, options: Vec<MacroOption<O>>) -> Choice<O> {
    let options = Table::Owned(options.into_iter().map(Entry::from).collect());
    Choice {
        instructions,
        options,
    }
}

// the derive has validated the table, so only `of` validates it again
fn options_of<O: Options>() -> Vec<MacroOption<O>> {
    O::OPTIONS
        .iter()
        .map(|option| {
            let description = option.description.map(|d| Json::String(Cow::Borrowed(d)));
            (option.name, description, (option.value)())
        })
        .collect()
}

impl<O> Entry<O> {
    fn new(
        name: Cow<'static, str>,
        description: Result<Option<Box<RawValue>>, InvalidJson>,
        value: O,
    ) -> Result<Self, InvalidChoice> {
        let description = match description {
            Ok(description) => description.map(Json::from),
            Err(error) => {
                let name = name.into_owned();
                return Err(InvalidChoice::Description { name, error });
            }
        };
        Ok(Entry {
            name,
            description,
            value,
        })
    }
}

impl<O> From<MacroOption<O>> for Entry<O> {
    fn from((name, description, value): MacroOption<O>) -> Self {
        let name = Cow::Borrowed(name);
        Entry {
            name,
            description,
            value,
        }
    }
}

impl<O> QuestionSet for Choice<O> {
    type AnswerSet = ChoiceAnswer<O>;
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
pub struct ChoiceAnswer<O> {
    question: Choice<O>,
    choice: usize,
    confidence: f64,
    probabilities: Vec<f64>,
}

impl<O> ChoiceAnswer<O> {
    #[must_use]
    pub fn question(&self) -> &Choice<O> {
        &self.question
    }

    #[must_use]
    pub fn into_question(self) -> Choice<O> {
        self.question
    }

    #[must_use]
    pub fn most_likely(&self) -> &O {
        &self.question.options.as_entries()[self.choice].value
    }

    #[must_use]
    pub fn into_most_likely(self) -> O {
        let mut options = self.question.options.into_entries();
        options.swap_remove(self.choice).value
    }

    pub fn top(&self, n: usize) -> impl Iterator<Item = (&O, f64)> {
        let mut ranked: Vec<_> = self.probabilities().collect();

        // a stable sort keeps equal options in the order of the question
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        ranked.into_iter().take(n)
    }

    #[must_use]
    pub fn confidence(&self) -> f64 {
        self.confidence
    }

    pub fn probabilities(&self) -> impl ExactSizeIterator<Item = (&O, f64)> {
        self.question
            .options
            .as_entries()
            .iter()
            .zip(&self.probabilities)
            .map(|(option, &probability)| (&option.value, probability))
    }

    #[must_use]
    pub fn into_probabilities(self) -> Vec<(O, f64)> {
        self.question
            .options
            .into_entries()
            .into_iter()
            .zip(self.probabilities)
            .map(|(option, probability)| (option.value, probability))
            .collect()
    }
}

impl<O> Serialize for ChoiceAnswer<O> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // the names, so a value needs no `Serialize`
        let options = self.question.options.as_entries();
        let names: Vec<_> = options.iter().map(|o| &o.name).collect();
        let mut answer = serializer.serialize_struct("ChoiceAnswer", 4)?;
        answer.serialize_field("most_likely", names[self.choice])?;
        answer.serialize_field("confidence", &self.confidence)?;
        answer.serialize_field("options", &names)?;
        answer.serialize_field("probabilities", &self.probabilities)?;
        answer.end()
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename = "choice")]
struct Question<'a, O> {
    instructions: &'a Json,
    #[serde(serialize_with = "criteria")]
    criteria: &'a [Entry<O>],
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

impl<O> Sealed for Choice<O> {
    fn write(&mut self, raw_question_set: &mut RawQuestionSet) {
        let instructions = &self.instructions;
        let criteria = self.options.entries();
        raw_question_set.push(&Question {
            instructions,
            criteria,
        });
    }

    fn read(
        mut self,
        raw_answer_set: &mut RawAnswerSet,
    ) -> Result<<Self as QuestionSet>::AnswerSet, String> {
        let Answer::Choice {
            choice,
            confidence,
            mut probabilities,
        } = raw_answer_set.next()?;
        let options = self.options.entries();
        let choice = options
            .iter()
            .position(|o| o.name == choice)
            .ok_or_else(|| format!("must choose an option of the Choice, not `{choice}`"))?;
        validate_range("confidence", confidence, 1.0)?;
        let count = probabilities.len();
        let probabilities = options
            .iter()
            .map(|o| probabilities.remove(o.name.as_ref()))
            .collect::<Option<Vec<_>>>()
            .filter(|_| count == options.len())
            .ok_or("must give a probability for each option of the Choice, and no other")?;
        for &probability in &probabilities {
            validate_range("probability", probability, 1.0)?;
        }
        let question = self;
        Ok(ChoiceAnswer {
            question,
            choice,
            confidence,
            probabilities,
        })
    }
}

// by hand, as a `serde_json::Map` would sort the options by name
fn criteria<O, S: Serializer>(options: &&[Entry<O>], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_map(options.iter().map(|o| (&o.name, &o.description)))
}

#[cfg(test)]
mod tests {
    use super::{Choice, ChoiceAnswer, ChoiceOption, InvalidChoice, Options};
    use crate::InvalidJson;
    use crate::ask::wire::{RawAnswerSet, RawQuestionSet, Sealed};
    use serde_json::{Map, Value, json};
    use std::collections::BTreeMap;

    fn serialized<O>(mut choice: Choice<O>) -> String {
        let mut raw_question_set = RawQuestionSet::default();
        choice.write(&mut raw_question_set);
        raw_question_set.texts()[0].to_owned()
    }

    fn read<O>(choice: Choice<O>, answer: Value) -> Result<ChoiceAnswer<O>, String> {
        choice.read(&mut RawAnswerSet::of(vec![answer]))
    }

    fn answer(choice: &str, probabilities: &Value) -> Value {
        json!({
            "type": "choice",
            "choice": choice,
            "confidence": 0.6,
            "probabilities": probabilities,
        })
    }

    // one type of option, so coverage sees each failure of `validated` and its success together
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
        read(choice, answer("go", &probabilities)).unwrap()
    }

    #[test]
    fn new_names_each_item_by_its_display() {
        let years = Choice::new("In which year?", 2026..=2027).unwrap();
        let languages = Choice::new("Which language?", ["rust", "python", "go"]).unwrap();

        assert_eq!(
            serialized(years),
            r#"{"type":"choice","instructions":"In which year?","criteria":{"2026":null,"2027":null}}"#
        );
        assert_eq!(
            serialized(languages),
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
                serialized(Choice::with_descriptions("What next?", pairs).unwrap()),
                r#"{"block":"Ask for a fix.","approve":"Approve it."}"#,
            ),
            (
                serialized(Choice::with_descriptions("What next?", &sorted).unwrap()),
                r#"{"approve":{"when":"tests pass"},"block":null}"#,
            ),
            (
                serialized(Choice::with_descriptions("What next?", &map).unwrap()),
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
        assert_eq!(criteria(by_id), r#"{"7":null,"3":null}}"#);
        assert_eq!(criteria(by_name), r#"{"search":null,"edit":null}}"#);
        assert_eq!(
            criteria(described),
            r#"{"search":"Find a file.","edit":null}}"#
        );
    }

    #[test]
    fn of_takes_the_options_of_the_type() {
        let choice = Choice::<Problem>::of("What is the worst problem?").unwrap();

        assert_eq!(
            serialized(choice),
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
    fn read_keeps_the_values_of_the_api() {
        let answer = languages();

        assert_eq!(*answer.most_likely(), "go");
        assert_eq!(answer.confidence(), 0.6);
        let probabilities: Vec<_> = answer.probabilities().collect();
        assert_eq!(
            probabilities,
            [(&"rust", 0.2), (&"python", 0.4), (&"go", 0.4)]
        );
        assert_eq!(answer.probabilities().len(), 3);
    }

    #[test]
    fn read_refuses_an_answer_that_does_not_fit() {
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
            (
                json!({
                    "type": "choice",
                    "choice": "go",
                    "confidence": 1.2,
                    "probabilities": { "rust": 0.5, "go": 0.5 },
                }),
                "must give a confidence from 0 to 1, not 1.2",
            ),
            (
                answer("go", &json!({ "go": 1.1, "rust": -0.1 })),
                "must give a probability from 0 to 1, not -0.1",
            ),
        ];
        for (json, message) in cases {
            let choice = Choice::new("Which language?", ["rust", "go"]).unwrap();
            assert_eq!(read(choice, json).unwrap_err(), message);
        }
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
    fn serialize_gives_the_names_and_the_probabilities_in_order() {
        // not `Serialize`, so the answer writes the names
        struct File(&'static str);
        let files = [File("main.rs"), File("lib.rs")];
        let choice = Choice::with_values("Which file?", files, |f| f.0).unwrap();
        let probabilities = json!({ "main.rs": 0.3, "lib.rs": 0.7 });
        let answer = read(choice, answer("lib.rs", &probabilities)).unwrap();

        assert_eq!(
            serde_json::to_string(&answer).unwrap(),
            concat!(
                r#"{"most_likely":"lib.rs","confidence":0.6,"#,
                r#""options":["main.rs","lib.rs"],"probabilities":[0.3,0.7]}"#,
            )
        );
    }

    #[test]
    fn into_methods_give_the_values_back() {
        assert_eq!(languages().into_most_likely(), "go");
        assert_eq!(
            languages().into_probabilities(),
            [("rust", 0.2), ("python", 0.4), ("go", 0.4)]
        );

        let question = serialized(languages().into_question());
        assert_eq!(serialized(languages().question().clone()), question);
        assert!(question.contains(r#""instructions":"Which language?""#));
    }
}
