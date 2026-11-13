use logos_tokens::contract::{ResolveRequest, ResolveResult, TokenSnapshot};
use logos_tokens::contract_resolver::resolve;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VectorFile {
    cases: Vec<VectorCase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VectorCase {
    id: String,
    input: VectorInput,
    #[serde(rename = "expectedOutput")]
    expected: ResolveResult,
}

#[derive(Deserialize)]
struct VectorInput {
    snapshot: TokenSnapshot,
    request: ResolveRequest,
}

#[test]
fn proposed_v01_resolution_vectors_match_exactly() {
    let vectors: VectorFile = serde_json::from_str(include_str!(
        "../../../contracts/design-token/v0.1/fixtures/resolution-vectors.json"
    ))
    .unwrap();
    for case in vectors.cases {
        let actual = resolve(&case.input.snapshot, &case.input.request);
        assert_eq!(
            actual,
            case.expected,
            "fixture {}\nactual: {}\nexpected: {}",
            case.id,
            serde_json::to_string(&actual).unwrap(),
            serde_json::to_string(&case.expected).unwrap()
        );
    }
}
