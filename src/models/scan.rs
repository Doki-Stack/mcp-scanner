//! Request/response types for `POST /mcp/v1/tools/scan`.
//!
//! Note: these are distinct from `doki_shared::models::scanner::{ScanInput,
//! ScanResult}` — that shared crate's types don't carry `org_id` or the
//! skill_md/instructions_md output paths the actual documented endpoint
//! contract requires, so this service defines its own rather than force-fit
//! a mismatched shared type.

use serde::{Deserialize, Serialize};

/// Input to `scan`: `{org_id, repo_url, branch, commit_sha}`.
#[derive(Debug, Clone, Deserialize)]
pub struct ScanRequest {
    pub org_id: String,
    pub repo_url: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub commit_sha: Option<String>,
}

/// Output of `scan`: MinIO paths to the generated artifacts plus timing.
#[derive(Debug, Clone, Serialize)]
pub struct ScanResponse {
    pub skill_md_path: String,
    pub instructions_md_path: String,
    pub schema_version: String,
    pub scan_duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_request_deserializes_required_fields_only() {
        let req: ScanRequest = serde_json::from_str(
            r#"{"org_id": "org-1", "repo_url": "https://github.com/acme/infra"}"#,
        )
        .unwrap();
        assert_eq!(req.org_id, "org-1");
        assert_eq!(req.repo_url, "https://github.com/acme/infra");
        assert_eq!(req.branch, None);
        assert_eq!(req.commit_sha, None);
    }

    #[test]
    fn scan_request_deserializes_all_fields() {
        let req: ScanRequest = serde_json::from_str(
            r#"{"org_id": "org-1", "repo_url": "https://github.com/acme/infra", "branch": "main", "commit_sha": "abc123"}"#,
        )
        .unwrap();
        assert_eq!(req.branch, Some("main".to_string()));
        assert_eq!(req.commit_sha, Some("abc123".to_string()));
    }

    #[test]
    fn scan_request_rejects_missing_required_field() {
        let result: Result<ScanRequest, _> =
            serde_json::from_str(r#"{"org_id": "org-1"}"#);
        assert!(result.is_err(), "repo_url is required, should fail without it");
    }

    #[test]
    fn scan_response_serializes_with_documented_field_names() {
        let resp = ScanResponse {
            skill_md_path: "org_id=org-1/repo/sha/skill.md".to_string(),
            instructions_md_path: "org_id=org-1/repo/sha/instructions.md".to_string(),
            schema_version: "1.0".to_string(),
            scan_duration_ms: 4200,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["skill_md_path"], "org_id=org-1/repo/sha/skill.md");
        assert_eq!(json["instructions_md_path"], "org_id=org-1/repo/sha/instructions.md");
        assert_eq!(json["schema_version"], "1.0");
        assert_eq!(json["scan_duration_ms"], 4200);
    }
}
