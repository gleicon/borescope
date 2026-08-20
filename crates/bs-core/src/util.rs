/// Returns true when `path` looks like a test file by convention.
/// Matches: test/ tests/ testdata/ __tests__/ spec/ fixtures/ directories,
/// and file suffixes _test.rs _test.go .test.ts .test.js .spec.ts .spec.js Test.java _spec.rb.
pub fn is_test_path(path: &str) -> bool {
    let p = path.replace('\\', "/");
    for seg in [
        "/test/",
        "/tests/",
        "/testdata/",
        "/__tests__/",
        "/spec/",
        "/fixtures/",
        "/mocks/",
    ] {
        if p.contains(seg) {
            return true;
        }
    }
    for prefix in ["test/", "tests/", "testdata/", "spec/", "fixtures/"] {
        if p.starts_with(prefix) {
            return true;
        }
    }
    for suffix in [
        "_test.rs",
        "_test.go",
        "_test.py",
        ".test.ts",
        ".test.tsx",
        ".test.js",
        ".spec.ts",
        ".spec.tsx",
        ".spec.js",
        "Test.java",
        "_spec.rb",
    ] {
        if p.ends_with(suffix) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_test_path_detects_directories() {
        assert!(is_test_path("tests/integration/auth.rs"));
        assert!(is_test_path("src/tests/mod.rs"));
        assert!(is_test_path("testdata/fixtures/sample.json"));
        assert!(is_test_path("__tests__/handler.test.ts"));
        assert!(is_test_path("spec/models/user_spec.rb"));
    }

    #[test]
    fn test_is_test_path_detects_suffixes() {
        assert!(is_test_path("src/auth/auth_test.rs"));
        assert!(is_test_path("src/auth_test.go"));
        assert!(is_test_path("src/handler.test.ts"));
        assert!(is_test_path("src/handler.spec.js"));
        assert!(is_test_path("src/AuthTest.java"));
        assert!(is_test_path("src/user_spec.rb"));
    }

    #[test]
    fn test_is_test_path_passes_production_files() {
        assert!(!is_test_path("src/auth.rs"));
        assert!(!is_test_path("src/http/router.rs"));
        assert!(!is_test_path("crates/bs-core/src/store.rs"));
        assert!(!is_test_path("src/latest_context.ts"));
        assert!(!is_test_path("src/protest.rs")); // "protest" contains "test" but is not a test
    }
}
