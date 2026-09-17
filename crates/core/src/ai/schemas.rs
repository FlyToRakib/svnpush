//! The JSON every AI task must answer with, and its validation (plan §9.7).
//!
//! Every answer is validated here whatever the dialect: structured-output
//! modes reduce malformed answers, they do not remove them.

use std::sync::LazyLock;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

/// `draft_release`: the next version and its release text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DraftAnswer {
    /// Suggested version.
    pub version: String,
    /// Why, in one sentence.
    pub reason: String,
    /// The changelog entry body.
    pub changelog_markdown: String,
    /// The upgrade notice, or an empty string.
    pub upgrade_notice: String,
    /// One paragraph about the release.
    pub summary: String,
}

/// `summarise_file`: one file's change in a sentence or two.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FileSummary {
    /// The file.
    pub path: String,
    /// What changed.
    pub summary: String,
}

/// One suggested fix: replace `original` with `replacement` in `path`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SuggestedFix {
    /// The failed check.
    pub check_id: String,
    /// The file to change.
    pub path: String,
    /// The exact text to replace, copied from the excerpt.
    pub original: String,
    /// The corrected text.
    pub replacement: String,
}

/// `explain_failures`: plain-language explanation and the smallest fixes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExplainAnswer {
    /// What failed and why, in plain language.
    pub explanation: String,
    /// Text fixes; empty when the fixes are in code.
    pub fixes: Vec<SuggestedFix>,
}

fn object(properties: &Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

/// Schema for [`DraftAnswer`].
pub static DRAFT_RELEASE: LazyLock<Value> = LazyLock::new(|| {
    let s = json!({ "type": "string" });
    object(
        &json!({ "version": s, "reason": s, "changelog_markdown": s, "upgrade_notice": s, "summary": s }),
        &["version", "reason", "changelog_markdown", "upgrade_notice", "summary"],
    )
});

/// Schema for [`FileSummary`].
pub static SUMMARISE_FILE: LazyLock<Value> = LazyLock::new(|| {
    let s = json!({ "type": "string" });
    object(&json!({ "path": s, "summary": s }), &["path", "summary"])
});

/// Schema for [`ExplainAnswer`].
pub static EXPLAIN_FAILURES: LazyLock<Value> = LazyLock::new(|| {
    let s = json!({ "type": "string" });
    let fix = object(
        &json!({ "check_id": s, "path": s, "original": s, "replacement": s }),
        &["check_id", "path", "original", "replacement"],
    );
    object(
        &json!({ "explanation": s, "fixes": { "type": "array", "items": fix } }),
        &["explanation", "fixes"],
    )
});

/// The JSON object inside a model's answer: the whole text, a fenced block,
/// or the outermost braces.
pub fn extract_json(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Some(value);
    }
    let unfenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|rest| rest.trim_end().strip_suffix("```"))
        .map(str::trim);
    if let Some(value) = unfenced.and_then(|t| serde_json::from_str(t).ok()) {
        return Some(value);
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    (start < end).then(|| serde_json::from_str(&trimmed[start..=end]).ok())?
}

/// Parses and validates an answer against `schema`. The error text is what a
/// retry sends back to the model.
pub fn parse_answer<T: DeserializeOwned>(text: &str, schema: &Value) -> Result<T, String> {
    let value = extract_json(text).ok_or_else(|| "The answer was not a JSON object.".to_owned())?;
    let validator =
        jsonschema::validator_for(schema).map_err(|e| format!("Invalid schema: {e}"))?;
    let problems: Vec<String> =
        validator.iter_errors(&value).map(|e| format!("{} at {}", e, e.instance_path())).collect();
    if !problems.is_empty() {
        return Err(format!("The JSON did not match the schema: {}.", problems.join("; ")));
    }
    serde_json::from_value(value).map_err(|e| format!("The JSON did not match the schema: {e}."))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{"version":"1.2.0","reason":"New features.","changelog_markdown":"* Added x.","upgrade_notice":"","summary":"Adds x."}"#;

    #[test]
    fn schemas_compile() {
        for schema in [&*DRAFT_RELEASE, &*SUMMARISE_FILE, &*EXPLAIN_FAILURES] {
            assert!(jsonschema::validator_for(schema).is_ok());
        }
    }

    #[test]
    fn extracts_plain_fenced_and_embedded_json() {
        assert!(extract_json(GOOD).is_some());
        assert!(extract_json(&format!("```json\n{GOOD}\n```")).is_some());
        assert!(extract_json(&format!("Here you go:\n{GOOD}\nThanks.")).is_some());
        assert!(extract_json("no json here").is_none());
    }

    #[test]
    fn validates_against_the_schema() {
        let draft: DraftAnswer = parse_answer(GOOD, &DRAFT_RELEASE).unwrap();
        assert_eq!(draft.version, "1.2.0");
        let missing =
            parse_answer::<DraftAnswer>(r#"{"version":"1.2.0"}"#, &DRAFT_RELEASE).unwrap_err();
        assert!(missing.contains("did not match"), "{missing}");
        let extra = GOOD.replace("\"summary\"", "\"extra\":1,\"summary\"");
        assert!(parse_answer::<DraftAnswer>(&extra, &DRAFT_RELEASE).is_err());
        assert_eq!(
            parse_answer::<DraftAnswer>("sorry", &DRAFT_RELEASE).unwrap_err(),
            "The answer was not a JSON object."
        );
    }

    #[test]
    fn explain_answer_parses() {
        let text = r#"{"explanation":"Stable tag is trunk.","fixes":[{"check_id":"V06","path":"readme.txt","original":"Stable tag: trunk","replacement":"Stable tag: 1.2.0"}]}"#;
        let answer: ExplainAnswer = parse_answer(text, &EXPLAIN_FAILURES).unwrap();
        assert_eq!(answer.fixes[0].check_id, "V06");
    }
}
