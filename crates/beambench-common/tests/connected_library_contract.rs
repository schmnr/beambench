use beambench_common::connected_library::{
    CRAFTGINEER_MAX_FILE_BYTES, LIBRARY_SCHEMA_VERSION, LibraryAccountResponse,
    LibraryCollectionsResponse, LibraryErrorResponse, LibraryListingResponse, LibraryProvider,
};
use serde_json::{Value, json};

// Canonical source: Craftgineer/src/lib/integrations/desktop-library/fixtures/v1.json.
// Keep this snapshot byte-identical when the two protocol implementations change.
fn fixtures() -> Value {
    serde_json::from_str(include_str!("fixtures/desktop_library_v1.json")).unwrap()
}

// Canonical source: PrintCutCarve/lib/integrations/desktop-library/fixtures/v1.json.
fn printcutcarve_fixtures() -> Value {
    serde_json::from_str(include_str!("fixtures/printcutcarve_library_v1.json")).unwrap()
}

#[test]
fn reads_printcutcarve_packs_originals_and_unavailable_entries() {
    let fixture = printcutcarve_fixtures();
    let provider = LibraryProvider::Printcutcarve;
    let account: LibraryAccountResponse =
        serde_json::from_value(fixture["accountResponse"].clone()).unwrap();
    assert_eq!(account.validate_for_provider(provider), Ok(()));
    assert_eq!(
        account.validate_for_provider(LibraryProvider::Craftgineer),
        Err("library_account_mismatch")
    );
    let listing: LibraryListingResponse =
        serde_json::from_value(fixture["listingResponse"].clone()).unwrap();
    assert_eq!(
        listing.validate_for_provider(provider, &account.account.id),
        Ok(())
    );
    assert_eq!(
        serde_json::to_value(&listing).unwrap(),
        fixture["listingResponse"]
    );
    let collections: LibraryCollectionsResponse =
        serde_json::from_value(fixture["collectionsResponse"].clone()).unwrap();
    assert_eq!(
        collections.validate_for(provider, &account.account.id),
        Ok(())
    );
    assert_eq!(
        collections.validate_for(provider, "other-account"),
        Err("library_account_mismatch")
    );
    assert_eq!(
        serde_json::to_value(collections).unwrap(),
        fixture["collectionsResponse"]
    );
}

#[test]
fn rejects_cross_pack_previews_unsafe_slugs_and_unexpected_printcutcarve_media() {
    let fixture = printcutcarve_fixtures();
    for field in ["collection", "mimeType"] {
        let mut value = fixture["listingResponse"].clone();
        value["files"][0][field] = if field == "collection" {
            Value::Null
        } else {
            json!("image/png")
        };
        let listing: LibraryListingResponse = serde_json::from_value(value).unwrap();
        assert!(
            listing
                .validate_for_provider(
                    LibraryProvider::Printcutcarve,
                    fixture["accountResponse"]["account"]["id"]
                        .as_str()
                        .unwrap()
                )
                .is_err()
        );
    }
    for field in ["slug", "preview"] {
        let mut collections: LibraryCollectionsResponse =
            serde_json::from_value(fixture["collectionsResponse"].clone()).unwrap();
        if field == "slug" {
            collections.collections[0].slug = "../foreign".to_owned();
        } else {
            collections.collections[0]
                .preview
                .as_mut()
                .unwrap()
                .collection
                .as_mut()
                .unwrap()
                .id = "another-pack".to_owned();
        }
        assert_eq!(
            collections.validate_for(LibraryProvider::Printcutcarve, &collections.account_id),
            Err("invalid_library_collection")
        );
    }
}

#[test]
fn reads_craftgineer_account_and_all_file_formats() {
    let fixture = fixtures();
    let account: LibraryAccountResponse =
        serde_json::from_value(fixture["accountResponse"].clone()).unwrap();
    assert_eq!(account.schema_version, LIBRARY_SCHEMA_VERSION);
    assert_eq!(account.validate(), Ok(()));
    assert_eq!(
        account.capabilities.max_file_bytes,
        CRAFTGINEER_MAX_FILE_BYTES
    );
    assert_eq!(
        serde_json::to_value(&account).unwrap(),
        fixture["accountResponse"]
    );
    for name in ["listingResponse", "emptyListingResponse"] {
        let listing: LibraryListingResponse =
            serde_json::from_value(fixture[name].clone()).unwrap();
        assert_eq!(listing.validate_for(&account.account.id), Ok(()));
        assert_eq!(serde_json::to_value(listing).unwrap(), fixture[name]);
    }
}

#[test]
fn rejects_unexpected_account_versions_and_capability_limits() {
    let mut response: LibraryAccountResponse =
        serde_json::from_value(fixtures()["accountResponse"].clone()).unwrap();
    response.capabilities.max_file_bytes = CRAFTGINEER_MAX_FILE_BYTES + 1;
    assert_eq!(response.validate(), Err("invalid_library_capabilities"));
    response.schema_version = 2;
    assert_eq!(response.validate(), Err("unsupported_library_version"));
}

#[test]
fn reads_the_providers_machine_readable_errors() {
    for fixture in fixtures()["errorResponses"].as_array().unwrap() {
        let response: LibraryErrorResponse =
            serde_json::from_value(fixture["body"].clone()).unwrap();
        assert_eq!(
            u64::from(response.error.code.http_status()),
            fixture["status"]
        );
    }
}

#[test]
fn rejects_a_listing_for_another_account_or_protocol_version() {
    let mut listing: LibraryListingResponse =
        serde_json::from_value(fixtures()["listingResponse"].clone()).unwrap();
    assert_eq!(
        listing.validate_for("other-account"),
        Err("library_account_mismatch")
    );
    listing.schema_version = 2;
    assert_eq!(
        listing.validate_for("account-paid"),
        Err("unsupported_library_version")
    );
}

#[test]
fn rejects_unsafe_or_unversioned_file_metadata() {
    for (field, value, expected) in [
        (
            "sizeBytes",
            json!(CRAFTGINEER_MAX_FILE_BYTES + 1),
            "library_file_too_large",
        ),
        ("revision", json!("latest"), "invalid_library_revision"),
        ("id", json!(""), "invalid_library_file_id"),
        ("id", json!("../another-file"), "invalid_library_file_id"),
        ("createdAt", json!("tomorrow"), "invalid_library_timestamp"),
    ] {
        let mut response = fixtures()["listingResponse"].clone();
        response["files"][0][field] = value;
        let listing: LibraryListingResponse = serde_json::from_value(response).unwrap();
        assert_eq!(listing.validate_for("account-paid"), Err(expected));
    }
    let mut response = fixtures()["listingResponse"].clone();
    response["files"][0]["mimeType"] = json!("text/html");
    assert!(serde_json::from_value::<LibraryListingResponse>(response).is_err());
}

#[test]
fn allows_additive_response_fields_without_accepting_unknown_media() {
    let mut response = fixtures()["listingResponse"].clone();
    response["futureMetadata"] = json!({"ignored": true});
    let listing: LibraryListingResponse = serde_json::from_value(response).unwrap();
    assert_eq!(listing.validate_for("account-paid"), Ok(()));
}
