//! D9: Precision gate for the rust_simple fixture.
//! Copies testdata/rust_simple into a TempDir, indexes it (no git), queries
//! call edges at confidence >= 0.7, and asserts precision >= 0.9.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn borescope() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.push("borescope");
    p
}

fn fixture_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // bs-cli
    p.pop(); // crates
    p.push("testdata");
    p.push("rust_simple");
    p
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&entry.path(), &dst_path);
        } else {
            std::fs::copy(entry.path(), dst_path).unwrap();
        }
    }
}

#[derive(serde::Deserialize)]
struct GroundTruth {
    edges: Vec<GtEdge>,
}

#[derive(serde::Deserialize)]
struct GtEdge {
    from: String,
    to: String,
}

#[derive(serde::Deserialize)]
struct JsonOutput {
    root: JsonNode,
}

#[derive(serde::Deserialize)]
struct JsonNode {
    name: String,
    children: Vec<JsonNode>,
    confidence: f32,
    #[serde(default)]
    external: bool,
}

fn collect_edges(node: &JsonNode, parent: Option<&str>, edges: &mut HashSet<(String, String)>) {
    for child in &node.children {
        if !child.external && child.confidence >= 0.7 {
            edges.insert((node.name.clone(), child.name.clone()));
            collect_edges(child, Some(&node.name), edges);
        }
    }
    let _ = parent;
}

#[test]
fn precision_gate_rust_simple() {
    let fixture = fixture_dir();
    assert!(
        fixture.exists(),
        "testdata/rust_simple must exist: {}",
        fixture.display()
    );

    let gt_path = fixture.join("ground_truth.json");
    let gt: GroundTruth =
        serde_json::from_str(&std::fs::read_to_string(&gt_path).unwrap()).unwrap();
    let expected: HashSet<(String, String)> = gt
        .edges
        .iter()
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect();

    // Copy fixture into a hermetic temp dir so .borescope/ never pollutes source tree
    let tmp = TempDir::new().unwrap();
    copy_dir(&fixture, tmp.path());
    let repo = tmp.path();

    let index_out = Command::new(borescope())
        .args(["index", "--no-git"])
        .arg("--repo")
        .arg(repo)
        .output()
        .expect("borescope index failed to launch");

    assert!(
        index_out.status.success(),
        "borescope index failed:\n{}",
        String::from_utf8_lossy(&index_out.stderr)
    );

    let paths_out = Command::new(borescope())
        .args(["-o", "json", "--depth", "5", "--min-confidence", "0.0"])
        .args(["paths", "alpha"])
        .arg("--repo")
        .arg(repo)
        .output()
        .expect("borescope paths failed to launch");

    assert!(
        paths_out.status.success(),
        "borescope paths failed:\n{}",
        String::from_utf8_lossy(&paths_out.stderr)
    );

    let json_str = String::from_utf8_lossy(&paths_out.stdout);
    let output: JsonOutput = serde_json::from_str(&json_str)
        .unwrap_or_else(|e| panic!("paths JSON parse failed: {e}\noutput:\n{json_str}"));

    let mut predicted: HashSet<(String, String)> = HashSet::new();
    collect_edges(&output.root, None, &mut predicted);

    if predicted.is_empty() {
        panic!(
            "No edges extracted at confidence >= 0.7. Extraction may be broken.\nFull output: {json_str}"
        );
    }

    let true_positives: usize = predicted.intersection(&expected).count();
    let precision = true_positives as f64 / predicted.len() as f64;

    assert!(
        precision >= 0.9,
        "Precision {:.2} < 0.9 on rust_simple fixture.\nPredicted: {:?}\nExpected: {:?}",
        precision,
        predicted,
        expected
    );
}
