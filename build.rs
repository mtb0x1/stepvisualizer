fn main() {
    //needed by webgpu, to activate web cfg shit,
    println!("cargo:rustc-cfg=web");

    // Fetch the short git commit hash
    let git_hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_HASH={}", git_hash);

    // Monotonically increasing DB schema version derived from total commit count.
    // Used as the IndexedDB `version` integer — guarantees forward-only versioning
    // without manual bumping. Falls back to 1 in environments without git.
    let commit_count = std::process::Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(1);
    println!("cargo:rustc-env=DB_COMMIT_VERSION={}", commit_count);
    // Re-run whenever HEAD or any ref changes (new commit / branch switch).
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
}
