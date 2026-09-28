//! Git-based metrics and `diff`, on throwaway repos built by scripting `git`.

use std::path::{Path, PathBuf};
use std::process::Command;

use deadhand_core::config::Config;
use deadhand_core::evidence::MetricKind;
use deadhand_core::git::GitCli;

/// A temp git repo removed on drop.
struct Repo(PathBuf);

impl Repo {
    fn new(name: &str) -> Repo {
        let dir = std::env::temp_dir().join(format!("deadhand-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let r = Repo(dir);
        r.git(&["init", "-q", "-b", "main"], 0);
        r
    }

    fn git(&self, args: &[&str], day: i64) {
        let date = format!("{} +0000", 1_700_000_000 + day * 86_400);
        let out = Command::new("git")
            .args(["-c", "user.name=Dev", "-c", "user.email=dev@example.com", "-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(&self.0)
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn write(&self, path: &str, body: &str) {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn commit(&self, msg: &str, day: i64) {
        self.git(&["add", "-A"], day);
        self.git(&["commit", "-q", "-m", msg], day);
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn orphan_flags_and_churn() {
    let repo = Repo::new("orphan");
    let bulk: String = (0..900).map(|i| format!("export const v{i} = {i};\n")).collect();
    repo.write("src/generated_like.ts", &bulk);
    repo.write("src/lib/core.ts", "export const core = 1;\n");
    for i in 0..5 {
        repo.write(&format!("src/user{i}.ts"), "import { core } from \"./lib/core\";\nexport const u = core;\n");
    }
    repo.commit("initial", 0);
    for day in 1..4 {
        repo.write(
            "src/user0.ts",
            &format!("import {{ core }} from \"./lib/core\";\nexport const u = core + {day};\n"),
        );
        repo.commit("edit", 300 + day);
    }

    let r = deadhand_core::analyze_with(repo.path(), &Config::default(), &GitCli).unwrap();
    assert!(r.summary.git_available);
    let orphan = r.metric(MetricKind::OrphanedCode).unwrap();
    assert!(orphan.available);

    let keys = |path: &str| -> Vec<String> {
        r.evidence
            .iter()
            .filter(|e| e.metric == MetricKind::OrphanedCode && e.path == path)
            .map(|e| e.key.clone())
            .collect()
    };
    assert_eq!(keys("src/generated_like.ts"), vec!["untouched+bulk"]);
    // Five dependents, untouched for 300 days, added in the same bulk commit.
    assert_eq!(keys("src/lib/core.ts"), vec!["untouched+bulk+critical-untouched"]);
    // Edited recently: not orphaned.
    assert!(keys("src/user0.ts").is_empty());
    let user0 = r.module("src/user0.ts").unwrap();
    assert_eq!(user0.raw[&MetricKind::OrphanedCode]["recent_commits"], 3.0);
}

#[test]
fn diff_against_previous_commit() {
    let repo = Repo::new("diff");
    repo.write("src/a.ts", "export const a = () => 1;\n");
    repo.write("src/b.ts", "import { a } from \"./a\";\nexport const b = () => a();\n");
    repo.commit("base", 0);
    // Introduce a cycle in the working tree.
    repo.write("src/a.ts", "import { b } from \"./b\";\nexport const a = () => (Math.random() > 2 ? b() : 1);\n");

    let d = deadhand_core::diff::diff_against(repo.path(), "HEAD", &Config::default(), &GitCli).unwrap();
    assert!(d.delta < 0.0, "delta {}", d.delta);
    assert!(d.new_evidence.iter().any(|e| e.key == "cycle" && e.path == "src/a.ts"));
    assert!(d.resolved_evidence.is_empty());

    // The temporary worktree is gone.
    let out = Command::new("git").args(["worktree", "list"]).current_dir(repo.path()).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).lines().count(), 1);
}

#[test]
fn diff_unknown_revision_is_an_error() {
    let repo = Repo::new("badrev");
    repo.write("src/a.ts", "export const a = 1;\n");
    repo.commit("base", 0);
    assert!(deadhand_core::diff::diff_against(repo.path(), "nope", &Config::default(), &GitCli).is_err());
}
