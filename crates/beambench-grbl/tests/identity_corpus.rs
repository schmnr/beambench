use std::collections::{BTreeMap, BTreeSet};

use beambench_grbl::{GrblFamilyIdentity, GrblFamilyIdentityDetector, GrblFamilyIdentityStatus};
use serde::Deserialize;

const CORPUS_JSON: &str = include_str!("fixtures/grbl_family_identity_v1.json");

#[derive(Debug, Deserialize)]
struct IdentityCorpus {
    schema_version: u16,
    description: String,
    cases: Vec<IdentityCase>,
}

#[derive(Debug, Deserialize)]
struct IdentityCase {
    id: String,
    provenance: Vec<FixtureProvenance>,
    lines: Vec<String>,
    expected: GrblFamilyIdentity,
}

#[derive(Debug, Deserialize)]
struct FixtureProvenance {
    kind: String,
    repository: String,
    commit: String,
    url: String,
    note: String,
}

fn load_corpus() -> IdentityCorpus {
    serde_json::from_str(CORPUS_JSON).expect("identity corpus must be valid JSON")
}

fn detect<'a>(lines: impl IntoIterator<Item = &'a str>) -> GrblFamilyIdentity {
    let mut detector = GrblFamilyIdentityDetector::default();
    for line in lines {
        detector.observe_line(line);
    }
    detector.identity()
}

#[test]
fn versioned_transcript_corpus_matches_expected_identities() {
    let corpus = load_corpus();
    assert_eq!(corpus.schema_version, 1);
    assert!(!corpus.description.trim().is_empty());
    assert!(corpus.cases.len() >= 10);

    for case in &corpus.cases {
        let actual = detect(case.lines.iter().map(String::as_str));
        assert_eq!(actual, case.expected, "fixture {}", case.id);
    }
}

#[test]
fn every_transcript_order_produces_the_same_identity() {
    let corpus = load_corpus();

    for case in &corpus.cases {
        let mut indices: Vec<_> = (0..case.lines.len()).collect();
        visit_permutations(&mut indices, case.lines.len(), &mut |order| {
            let actual = detect(order.iter().map(|index| case.lines[*index].as_str()));
            assert_eq!(
                actual, case.expected,
                "fixture {} changed for order {order:?}",
                case.id
            );
        });
    }
}

#[test]
fn identified_build_versions_flow_to_positive_identity_without_coalescing() {
    let corpus = load_corpus();
    let mut versions = BTreeMap::new();

    for case in &corpus.cases {
        let identity = detect(case.lines.iter().map(String::as_str));
        let positive = identity.positive_identity();
        if identity.status == GrblFamilyIdentityStatus::Identified {
            let positive = positive
                .unwrap_or_else(|| panic!("fixture {} should produce positive identity", case.id));
            assert_eq!(
                positive.firmware_version, identity.firmware_version,
                "fixture {} lost its normalized build version",
                case.id
            );
            versions.insert(case.id.as_str(), positive.firmware_version);
        } else {
            assert!(
                positive.is_none(),
                "fixture {} unexpectedly became positive",
                case.id
            );
        }
    }

    assert_ne!(
        versions["fluid_nc_branch_hash_build"],
        versions["fluid_nc_dirty_branch_build"]
    );
    assert_ne!(
        versions["fluid_nc_current_identified"],
        versions["fluid_nc_bluetooth_variant"]
    );
    assert_eq!(versions["fluid_nc_nogit_build"], None);
    assert_eq!(versions["fluid_nc_placeholder_release"], None);
    assert_eq!(versions["grbl_hal_malformed_build_unbindable"], None);
}

#[test]
fn fixture_provenance_is_pinned_and_case_ids_are_unique() {
    let corpus = load_corpus();
    let mut ids = BTreeSet::new();

    for case in &corpus.cases {
        assert!(ids.insert(&case.id), "duplicate fixture id {}", case.id);
        assert!(
            !case.provenance.is_empty(),
            "fixture {} lacks provenance",
            case.id
        );

        for source in &case.provenance {
            assert!(!source.kind.trim().is_empty(), "fixture {} kind", case.id);
            assert!(
                !source.repository.trim().is_empty(),
                "fixture {} repository",
                case.id
            );
            assert!(!source.note.trim().is_empty(), "fixture {} note", case.id);
            assert_eq!(source.commit.len(), 40, "fixture {} commit", case.id);
            assert!(
                source.commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "fixture {} commit is not hexadecimal",
                case.id
            );
            assert!(
                source.url.contains(&format!("/blob/{}/", source.commit)),
                "fixture {} source is not pinned to its commit",
                case.id
            );
        }
    }
}

fn visit_permutations(indices: &mut [usize], size: usize, visitor: &mut impl FnMut(&[usize])) {
    if size <= 1 {
        visitor(indices);
        return;
    }

    visit_permutations(indices, size - 1, visitor);
    for index in 0..(size - 1) {
        if size.is_multiple_of(2) {
            indices.swap(index, size - 1);
        } else {
            indices.swap(0, size - 1);
        }
        visit_permutations(indices, size - 1, visitor);
    }
}
