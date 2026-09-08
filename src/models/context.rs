//! Request/response types for `POST /mcp/v1/tools/get-context`.

use serde::{Deserialize, Serialize};

/// Input to `get-context`: `{org_id, repo_url, branch}`.
#[derive(Debug, Clone, Deserialize)]
pub struct GetContextRequest {
    pub org_id: String,
    pub repo_url: String,
    #[serde(default)]
    pub branch: Option<String>,
}

/// Output of `get-context`. `stale` is true when the most recent scan
/// attempt for this repo failed, so these paths are from an earlier
/// successful scan rather than the latest commit — callers should treat
/// that as a signal to consider triggering a fresh scan, not as an error.
#[derive(Debug, Clone, Serialize)]
pub struct GetContextResponse {
    pub skill_md_path: String,
    pub instructions_md_path: String,
    pub schema_version: String,
    pub stale: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_context_request_deserializes_without_branch() {
        let req: GetContextRequest = serde_json::from_str(
            r#"{"org_id": "org-1", "repo_url": "https://github.com/acme/infra"}"#,
        )
        .unwrap();
        assert_eq!(req.org_id, "org-1");
        assert_eq!(req.branch, None);
    }

    #[test]
    fn get_context_response_reports_stale_flag() {
        let resp = GetContextResponse {
            skill_md_path: "path/skill.md".to_string(),
            instructions_md_path: "path/instructions.md".to_string(),
            schema_version: "1.0".to_string(),
            stale: true,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["stale"], true);
    }
}
