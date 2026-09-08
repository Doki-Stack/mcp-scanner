//! Walks a checked-out repo and produces a structured analysis: detected
//! technologies, Terraform resources, and the dominant file-naming
//! convention. This feeds the LLM summarizer (next task) as context.
//!
//! Terraform resource extraction is regex-based (`resource "type" "name"`),
//! not a real HCL parse — sufficient to hand the LLM a resource inventory,
//! not a substitute for `terraform validate`. Naming-convention detection
//! is a simple heuristic (kebab/snake/camel vote over file/dir basenames),
//! not a hard guarantee about the repo's actual conventions.

use std::path::Path;

use doki_shared::error::{Error, Result};
use regex::Regex;
use serde::Serialize;
use walkdir::WalkDir;

const IGNORED_DIRS: &[&str] = &[".git", "node_modules", "target", "vendor", ".terraform"];

/// A single `resource "type" "name" { ... }` block found in a `.tf` file.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TerraformResource {
    pub resource_type: String,
    pub resource_name: String,
    pub file_path: String,
}

/// Naming convention observed across file/directory basenames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NamingConvention {
    KebabCase,
    SnakeCase,
    CamelCase,
    Mixed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Analysis {
    /// Detected technologies, e.g. ["terraform", "docker", "nodejs"]. Sorted, deduplicated.
    pub technologies: Vec<String>,
    pub terraform_resources: Vec<TerraformResource>,
    pub naming_convention: NamingConvention,
    pub file_count: usize,
}

pub struct Analyzer;

impl Analyzer {
    /// Walks `root` synchronously; callers should run this via
    /// `spawn_blocking` if called from an async context with a large repo,
    /// same as the cloner does for git2.
    pub fn analyze(root: &Path) -> Result<Analysis> {
        let tf_resource_re = Regex::new(r#"resource\s+"([^"]+)"\s+"([^"]+)"\s*\{"#)
            .map_err(|e| Error::internal(format!("compile terraform resource regex: {e}")))?;

        let mut technologies = std::collections::BTreeSet::new();
        let mut terraform_resources = Vec::new();
        let mut naming_votes = NamingVotes::default();
        let mut file_count = 0usize;

        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| !is_ignored_dir(e))
        {
            let entry = entry.map_err(|e| Error::internal(format!("walk repo tree: {e}")))?;
            if !entry.file_type().is_file() {
                continue;
            }
            file_count += 1;

            let file_name = entry.file_name().to_string_lossy().to_string();
            naming_votes.observe(&file_name);

            detect_technology(&file_name).map(|tech| technologies.insert(tech.to_string()));

            if file_name.ends_with(".tf") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    let rel_path = entry
                        .path()
                        .strip_prefix(root)
                        .unwrap_or(entry.path())
                        .to_string_lossy()
                        .to_string();
                    for cap in tf_resource_re.captures_iter(&content) {
                        terraform_resources.push(TerraformResource {
                            resource_type: cap[1].to_string(),
                            resource_name: cap[2].to_string(),
                            file_path: rel_path.clone(),
                        });
                    }
                }
            }
        }

        Ok(Analysis {
            technologies: technologies.into_iter().collect(),
            terraform_resources,
            naming_convention: naming_votes.dominant(),
            file_count,
        })
    }
}

fn is_ignored_dir(entry: &walkdir::DirEntry) -> bool {
    entry.file_type().is_dir()
        && entry
            .file_name()
            .to_str()
            .map(|name| IGNORED_DIRS.contains(&name))
            .unwrap_or(false)
}

fn detect_technology(file_name: &str) -> Option<&'static str> {
    match file_name {
        "package.json" => Some("nodejs"),
        "Cargo.toml" => Some("rust"),
        "go.mod" => Some("go"),
        "requirements.txt" | "pyproject.toml" | "Pipfile" => Some("python"),
        "docker-compose.yml" | "docker-compose.yaml" => Some("docker-compose"),
        "Gemfile" => Some("ruby"),
        "pom.xml" => Some("java"),
        "build.gradle" | "build.gradle.kts" => Some("java"),
        _ if file_name == "Dockerfile" || file_name.starts_with("Dockerfile.") => Some("docker"),
        _ if file_name.ends_with(".tf") || file_name.ends_with(".tfvars") => Some("terraform"),
        _ if file_name.ends_with(".yml") && (file_name.contains("k8s") || file_name.contains("kustomization")) => {
            Some("kubernetes")
        }
        _ => None,
    }
}

/// Tallies kebab-case / snake_case / camelCase votes across basenames to
/// find the repo's dominant naming convention. A name only votes if it's
/// unambiguous (has a separator, or an internal capital); an extension-only
/// or single-word name like "README" or "Dockerfile" doesn't vote.
#[derive(Default)]
struct NamingVotes {
    kebab: u32,
    snake: u32,
    camel: u32,
}

impl NamingVotes {
    fn observe(&mut self, file_name: &str) {
        let stem = file_name.split('.').next().unwrap_or(file_name);
        if stem.is_empty() {
            return;
        }
        let has_hyphen = stem.contains('-');
        let has_underscore = stem.contains('_');
        let has_internal_upper = stem
            .chars()
            .skip(1)
            .any(|c| c.is_ascii_uppercase());

        match (has_hyphen, has_underscore, has_internal_upper) {
            (true, false, _) => self.kebab += 1,
            (false, true, _) => self.snake += 1,
            (false, false, true) => self.camel += 1,
            _ => {} // ambiguous or single-word: no vote
        }
    }

    fn dominant(&self) -> NamingConvention {
        let total = self.kebab + self.snake + self.camel;
        if total == 0 {
            return NamingConvention::Mixed;
        }
        let max = self.kebab.max(self.snake).max(self.camel);
        if (max as f64) / (total as f64) <= 0.5 {
            return NamingConvention::Mixed;
        }
        if max == self.kebab {
            NamingConvention::KebabCase
        } else if max == self.snake {
            NamingConvention::SnakeCase
        } else {
            NamingConvention::CamelCase
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &Path, rel_path: &str, content: &str) {
        let full = dir.join(rel_path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }

    #[test]
    fn detects_multiple_technologies() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "package.json", "{}");
        write(dir.path(), "Dockerfile", "FROM alpine");
        write(dir.path(), "main.tf", "");
        write(dir.path(), "go.mod", "module example.com/x");

        let analysis = Analyzer::analyze(dir.path()).unwrap();

        assert_eq!(
            analysis.technologies,
            vec!["docker", "go", "nodejs", "terraform"]
        );
    }

    #[test]
    fn extracts_terraform_resources() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "main.tf",
            r#"
resource "aws_s3_bucket" "artifacts" {
  bucket = "my-artifacts"
}

resource "aws_instance" "web" {
  ami = "ami-123"
}
"#,
        );

        let analysis = Analyzer::analyze(dir.path()).unwrap();

        assert_eq!(analysis.terraform_resources.len(), 2);
        assert!(analysis.terraform_resources.contains(&TerraformResource {
            resource_type: "aws_s3_bucket".to_string(),
            resource_name: "artifacts".to_string(),
            file_path: "main.tf".to_string(),
        }));
        assert!(analysis.terraform_resources.contains(&TerraformResource {
            resource_type: "aws_instance".to_string(),
            resource_name: "web".to_string(),
            file_path: "main.tf".to_string(),
        }));
    }

    #[test]
    fn ignores_git_and_dependency_directories() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".git/HEAD", "ref: refs/heads/main");
        write(dir.path(), "node_modules/pkg/index.js", "module.exports = {}");
        write(dir.path(), "target/debug/mcp-scanner", "binary");
        write(dir.path(), "src/main.rs", "fn main() {}");

        let analysis = Analyzer::analyze(dir.path()).unwrap();

        // Only src/main.rs should be counted; .git, node_modules, target are skipped.
        assert_eq!(analysis.file_count, 1);
    }

    #[test]
    fn detects_kebab_case_naming_convention() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "user-service.go", "");
        write(dir.path(), "api-gateway.go", "");
        write(dir.path(), "data-store.go", "");

        let analysis = Analyzer::analyze(dir.path()).unwrap();

        assert_eq!(analysis.naming_convention, NamingConvention::KebabCase);
    }

    #[test]
    fn detects_snake_case_naming_convention() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "user_service.py", "");
        write(dir.path(), "api_gateway.py", "");
        write(dir.path(), "data_store.py", "");

        let analysis = Analyzer::analyze(dir.path()).unwrap();

        assert_eq!(analysis.naming_convention, NamingConvention::SnakeCase);
    }

    #[test]
    fn mixed_naming_convention_when_no_majority() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "user-service.go", "");
        write(dir.path(), "api_gateway.py", "");
        write(dir.path(), "dataStore.js", "");

        let analysis = Analyzer::analyze(dir.path()).unwrap();

        assert_eq!(analysis.naming_convention, NamingConvention::Mixed);
    }

    #[test]
    fn empty_repo_produces_empty_analysis() {
        let dir = tempfile::tempdir().unwrap();
        let analysis = Analyzer::analyze(dir.path()).unwrap();

        assert!(analysis.technologies.is_empty());
        assert!(analysis.terraform_resources.is_empty());
        assert_eq!(analysis.file_count, 0);
        assert_eq!(analysis.naming_convention, NamingConvention::Mixed);
    }
}
