use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DetectionResult {
    pub languages: Vec<String>,
    pub build_systems: Vec<String>,
    pub test_commands: Vec<String>,
    pub agent_guidance: Vec<String>,
}

/// Detect languages, build systems, and likely test commands from manifests and extensions.
pub fn detect_project(root: &Path, inventoried_relative_paths: &[String]) -> DetectionResult {
    let mut languages = BTreeSet::new();
    let mut build_systems = BTreeSet::new();
    let mut test_commands = BTreeSet::new();
    let mut agent_guidance = BTreeSet::new();

    let names: BTreeSet<String> = inventoried_relative_paths
        .iter()
        .map(|p| p.replace('\\', "/"))
        .collect();

    let has = |name: &str| {
        names
            .iter()
            .any(|p| p == name || p.ends_with(&format!("/{name}")))
    };
    let has_ext = |ext: &str| {
        names.iter().any(|p| {
            Path::new(p)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case(ext))
                .unwrap_or(false)
        })
    };

    if has("package.json") {
        languages.insert("javascript".into());
        build_systems.insert("npm".into());
        if let Some(cmds) = read_npm_scripts(root.join("package.json")) {
            for cmd in cmds {
                test_commands.insert(cmd);
            }
        } else {
            test_commands.insert("npm test".into());
        }
        if has("tsconfig.json") || has_ext("ts") || has_ext("tsx") {
            languages.insert("typescript".into());
        }
    }
    if has("Cargo.toml") {
        languages.insert("rust".into());
        build_systems.insert("cargo".into());
        test_commands.insert("cargo test".into());
    }
    if has("pyproject.toml") || has("requirements.txt") || has("setup.py") {
        languages.insert("python".into());
        build_systems.insert("pip".into());
        if has("pytest.ini")
            || names
                .iter()
                .any(|p| p.contains("test_") || p.contains("/tests/"))
        {
            test_commands.insert("pytest".into());
        } else {
            test_commands.insert("python -m unittest".into());
        }
    }
    if has("go.mod") {
        languages.insert("go".into());
        build_systems.insert("go".into());
        test_commands.insert("go test ./...".into());
    }
    if has("pom.xml") {
        languages.insert("java".into());
        build_systems.insert("maven".into());
        test_commands.insert("mvn test".into());
    }
    if has("build.gradle") || has("build.gradle.kts") {
        languages.insert("java".into());
        build_systems.insert("gradle".into());
        test_commands.insert("gradle test".into());
    }
    if has("Gemfile") {
        languages.insert("ruby".into());
        build_systems.insert("bundler".into());
        test_commands.insert("bundle exec rake test".into());
    }
    if has_ext("csproj") || has_ext("fsproj") || has_ext("sln") {
        languages.insert("csharp".into());
        build_systems.insert("dotnet".into());
        test_commands.insert("dotnet test".into());
    }
    if has_ext("rs") {
        languages.insert("rust".into());
    }
    if has_ext("py") {
        languages.insert("python".into());
    }
    if has_ext("go") {
        languages.insert("go".into());
    }
    if has_ext("md") && languages.is_empty() && build_systems.is_empty() {
        // notes-only material
    }

    for guidance in [
        "AGENTS.md",
        "CLAUDE.md",
        ".cursorrules",
        "CURSOR.md",
        ".github/copilot-instructions.md",
    ] {
        if has(guidance) || names.iter().any(|p| p.ends_with(guidance)) {
            agent_guidance.insert(guidance.to_string());
        }
    }

    DetectionResult {
        languages: languages.into_iter().collect(),
        build_systems: build_systems.into_iter().collect(),
        test_commands: test_commands.into_iter().collect(),
        agent_guidance: agent_guidance.into_iter().collect(),
    }
}

fn read_npm_scripts(path: std::path::PathBuf) -> Option<Vec<String>> {
    let text = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let scripts = value.get("scripts")?.as_object()?;
    let mut cmds = Vec::new();
    for key in ["test", "test:unit", "test:all", "check"] {
        if scripts.contains_key(key) {
            cmds.push(format!("npm run {key}"));
        }
    }
    if cmds.is_empty() && scripts.contains_key("test") {
        cmds.push("npm test".into());
    }
    if cmds.is_empty() {
        None
    } else {
        Some(cmds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn detects_npm_typescript_project() {
        let dir = tempdir().unwrap();
        let mut pkg = fs::File::create(dir.path().join("package.json")).unwrap();
        write!(
            pkg,
            r#"{{"name":"demo","scripts":{{"test":"vitest run","build":"tsc"}}}}"#
        )
        .unwrap();
        fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();
        let paths = vec![
            "package.json".into(),
            "tsconfig.json".into(),
            "src/main.ts".into(),
        ];
        let result = detect_project(dir.path(), &paths);
        assert!(result.languages.contains(&"typescript".to_string()));
        assert!(result.build_systems.contains(&"npm".to_string()));
        assert!(result
            .test_commands
            .iter()
            .any(|c| c.contains("npm run test")));
    }

    #[test]
    fn detects_cargo_project() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        let result = detect_project(dir.path(), &["Cargo.toml".into(), "src/lib.rs".into()]);
        assert_eq!(result.languages, vec!["rust".to_string()]);
        assert_eq!(result.build_systems, vec!["cargo".to_string()]);
        assert_eq!(result.test_commands, vec!["cargo test".to_string()]);
    }
}
