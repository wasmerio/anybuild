//! Performance benchmarks for shipit-cli
//!
//! Run with: cargo bench

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use shipit::providers::{
    go::GoProvider, node::NodeStaticProvider, php::PhpProvider, python::PythonProvider,
    staticfile::StaticfileProvider, Provider,
};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn bench_provider_detection(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create test files for different providers
    fs::write(project_path.join("package.json"), r#"{"name": "test"}"#).unwrap();
    fs::write(project_path.join("requirements.txt"), "flask").unwrap();
    fs::write(project_path.join("composer.json"), "{}").unwrap();
    fs::write(project_path.join("go.mod"), "module test").unwrap();
    fs::write(project_path.join("index.html"), "<html></html>").unwrap();

    c.bench_function("detect_node_provider", |b| {
        b.iter(|| {
            let node = NodeStaticProvider;
            black_box(node.detect(project_path))
        });
    });

    c.bench_function("detect_python_provider", |b| {
        b.iter(|| {
            let python = PythonProvider;
            black_box(python.detect(project_path))
        });
    });

    c.bench_function("detect_php_provider", |b| {
        b.iter(|| {
            let php = PhpProvider;
            black_box(php.detect(project_path))
        });
    });

    c.bench_function("detect_go_provider", |b| {
        b.iter(|| {
            let go = GoProvider;
            black_box(go.detect(project_path))
        });
    });

    c.bench_function("detect_staticfile_provider", |b| {
        b.iter(|| {
            let staticfile = StaticfileProvider;
            black_box(staticfile.detect(project_path))
        });
    });
}

fn bench_path_operations(c: &mut Criterion) {
    c.bench_function("pathbuf_join", |b| {
        let base = PathBuf::from("/tmp/test");
        b.iter(|| black_box(base.join("subdir").join("file.txt")));
    });

    c.bench_function("path_exists_check", |b| {
        let path = PathBuf::from("/tmp");
        b.iter(|| black_box(path.exists()));
    });
}

fn bench_string_operations(c: &mut Criterion) {
    c.bench_function("shlex_split_simple", |b| {
        let cmd = "npm run build --production";
        b.iter(|| black_box(shlex::split(cmd)));
    });

    c.bench_function("shlex_split_complex", |b| {
        let cmd = r#"sh -c "echo 'hello world' && npm install""#;
        b.iter(|| black_box(shlex::split(cmd)));
    });

    c.bench_function("format_port_string", |b| {
        let port = 8080u16;
        b.iter(|| black_box(format!("http://localhost:{}", port)));
    });
}

fn bench_file_operations(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "test content").unwrap();

    c.bench_function("read_small_file", |b| {
        b.iter(|| black_box(fs::read_to_string(&test_file).unwrap()));
    });

    c.bench_function("file_exists_check", |b| {
        b.iter(|| black_box(test_file.exists()));
    });
}

fn bench_json_parsing(c: &mut Criterion) {
    c.bench_function("parse_package_json", |b| {
        let json = r#"{"name": "test", "version": "1.0.0", "dependencies": {"react": "^18.0.0"}}"#;
        b.iter(|| black_box(serde_json::from_str::<serde_json::Value>(json).unwrap()));
    });

    c.bench_function("parse_simple_json", |b| {
        let json = r#"{"key": "value"}"#;
        b.iter(|| black_box(serde_json::from_str::<serde_json::Value>(json).unwrap()));
    });
}

criterion_group!(
    benches,
    bench_provider_detection,
    bench_path_operations,
    bench_string_operations,
    bench_file_operations,
    bench_json_parsing
);

criterion_main!(benches);
