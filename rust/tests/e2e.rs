use rand::Rng;
use regex::Regex;
use reqwest::Client;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::task;

#[derive(Clone, Copy)]
enum BuildMode {
    Local,
    Wasmer,
    Docker,
}

#[derive(Clone)]
struct HttpRequest {
    path: &'static str,
    body_match: &'static str,
    method: &'static str,
}

#[derive(Clone)]
struct E2eCase {
    path: &'static str,
    serve_pattern: &'static str,
    http: Vec<HttpRequest>,
    use_random_port: bool,
}

macro_rules! e2e_test {
    ($case:ident, $mode:ident) => {
        paste::paste! {
            #[tokio::test]
            async fn [<test_e2e_ $case:snake _ $mode:snake>]() {
                let case = $case();
                let mode = BuildMode::$mode;
                run_e2e_test(case, mode).await;
            }
        }
    };
}

fn php_nobuild() -> E2eCase {
    E2eCase {
        path: "examples/php-nobuild",
        serve_pattern: r"PHP 8\.3\.[0-9]+ Development Server \(http://localhost:[\d]+\) started",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"PHP Version 8\.3\.[0-9]+",
            method: "GET",
        }],
        use_random_port: true,
    }
}

fn php_nobuild_no_random_port() -> E2eCase {
    E2eCase {
        path: "examples/php-nobuild",
        serve_pattern: r"PHP 8\.3\.[0-9]+ Development Server \(http://localhost:[\d]+\) started",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"PHP Version 8\.3\.[0-9]+",
            method: "GET",
        }],
        use_random_port: false,
    }
}

fn php_api() -> E2eCase {
    E2eCase {
        path: "examples/php-api",
        serve_pattern: r"PHP 8\.3\.[0-9]+ Development Server \(http://localhost:[\d]+\) started",
        http: vec![
            HttpRequest {
                path: "/",
                body_match: r#""version" \s* : \s* "8\.3\.[0-9]+""#,
                method: "GET",
            },
            HttpRequest {
                path: "/api/greet/Alice",
                body_match: r"Hello, Alice!",
                method: "GET",
            },
        ],
        use_random_port: true,
    }
}

fn php_wordpress() -> E2eCase {
    E2eCase {
        path: "examples/php-wordpress",
        serve_pattern: r"PHP 8\.3\.[0-9]+ Development Server \(http://localhost:[\d]+\) started",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"WordPress",
            method: "GET",
        }],
        use_random_port: true,
    }
}

fn static_nobuild() -> E2eCase {
    E2eCase {
        path: "examples/static-nobuild",
        serve_pattern: r"server is listening on",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"Test",
            method: "GET",
        }],
        use_random_port: true,
    }
}

fn staticfile() -> E2eCase {
    E2eCase {
        path: "examples/staticfile",
        serve_pattern: r"server is listening on",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"Hello from static site!",
            method: "GET",
        }],
        use_random_port: true,
    }
}

fn hugo() -> E2eCase {
    E2eCase {
        path: "examples/hugo",
        serve_pattern: r"server is listening on",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"My New Hugo Site",
            method: "GET",
        }],
        use_random_port: true,
    }
}

fn mkdocs() -> E2eCase {
    E2eCase {
        path: "examples/mkdocs",
        serve_pattern: r"server is listening on",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"Welcome to MkDocs",
            method: "GET",
        }],
        use_random_port: true,
    }
}

fn mkdocs_with_plugins() -> E2eCase {
    E2eCase {
        path: "examples/mkdocs-with-plugins",
        serve_pattern: r"server is listening on",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"Welcome to MkDocs with Plugins",
            method: "GET",
        }],
        use_random_port: true,
    }
}

fn python_fastapi() -> E2eCase {
    E2eCase {
        path: "examples/python-fastapi",
        serve_pattern: r"Uvicorn running on .*",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"Hello World from fastapi!",
            method: "GET",
        }],
        use_random_port: true,
    }
}

fn python_flask() -> E2eCase {
    E2eCase {
        path: "examples/python-flask",
        serve_pattern: r"Uvicorn running on .*",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"Welcome to Flask",
            method: "GET",
        }],
        use_random_port: true,
    }
}

fn python_django() -> E2eCase {
    E2eCase {
        path: "examples/python-django",
        serve_pattern: r"Uvicorn running on .*",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"Django",
            method: "GET",
        }],
        use_random_port: true,
    }
}

fn python_ffmpeg() -> E2eCase {
    E2eCase {
        path: "examples/python-ffmpeg",
        serve_pattern: r"Uvicorn running on .*",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"Take screenshot at 1s",
            method: "GET",
        }],
        use_random_port: true,
    }
}

fn python_pillow() -> E2eCase {
    E2eCase {
        path: "examples/python-pillow",
        serve_pattern: r"Uvicorn running on .*",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"Image Crop\s*&\s*Rotate",
            method: "GET",
        }],
        use_random_port: true,
    }
}

fn python_procfile() -> E2eCase {
    E2eCase {
        path: "examples/python-procfile",
        serve_pattern: r"Serving HTTP on .*",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"Test",
            method: "GET",
        }],
        use_random_port: true,
    }
}

fn python_streamlit() -> E2eCase {
    E2eCase {
        path: "examples/python-streamlit",
        serve_pattern: r".*You can now view your Streamlit app in your browser.*",
        http: vec![HttpRequest {
            path: "/",
            body_match: r"Streamlit",
            method: "GET",
        }],
        use_random_port: true,
    }
}

fn python_pandoc() -> E2eCase {
    E2eCase {
        path: "examples/python-pandoc",
        serve_pattern: r"Uvicorn running on .*",
        http: vec![],
        use_random_port: true,
    }
}

// Now generate tests for combinations
e2e_test!(php_nobuild, Local);
e2e_test!(php_nobuild, Wasmer);
e2e_test!(php_nobuild, Docker);
e2e_test!(php_nobuild_no_random_port, Local);
e2e_test!(php_api, Local);
e2e_test!(php_api, Wasmer);
e2e_test!(php_api, Docker);
e2e_test!(php_wordpress, Local);
e2e_test!(php_wordpress, Wasmer);
e2e_test!(php_wordpress, Docker);
e2e_test!(static_nobuild, Local);
e2e_test!(static_nobuild, Wasmer);
e2e_test!(static_nobuild, Docker);
e2e_test!(staticfile, Local);
e2e_test!(staticfile, Wasmer);
e2e_test!(staticfile, Docker);
e2e_test!(hugo, Local);
e2e_test!(hugo, Wasmer);
e2e_test!(hugo, Docker);
e2e_test!(mkdocs, Local);
e2e_test!(mkdocs, Wasmer);
e2e_test!(mkdocs, Docker);
e2e_test!(mkdocs_with_plugins, Local);
e2e_test!(mkdocs_with_plugins, Wasmer);
e2e_test!(mkdocs_with_plugins, Docker);
e2e_test!(python_fastapi, Local);
e2e_test!(python_fastapi, Wasmer);
e2e_test!(python_fastapi, Docker);
e2e_test!(python_flask, Local);
e2e_test!(python_flask, Wasmer);
e2e_test!(python_flask, Docker);
e2e_test!(python_django, Local);
e2e_test!(python_django, Wasmer);
e2e_test!(python_django, Docker);
e2e_test!(python_ffmpeg, Local);
e2e_test!(python_ffmpeg, Wasmer);
e2e_test!(python_ffmpeg, Docker);
e2e_test!(python_pillow, Local);
e2e_test!(python_pillow, Wasmer);
e2e_test!(python_pillow, Docker);
e2e_test!(python_procfile, Local);
e2e_test!(python_procfile, Wasmer);
e2e_test!(python_procfile, Docker);
e2e_test!(python_streamlit, Local);
e2e_test!(python_streamlit, Wasmer);
e2e_test!(python_streamlit, Docker);
e2e_test!(python_pandoc, Local);
e2e_test!(python_pandoc, Wasmer);
e2e_test!(python_pandoc, Docker);

fn get_free_port() -> u16 {
    loop {
        let port = rand::thread_rng().gen_range(1024..65535);
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
}

async fn run_e2e_test(case: E2eCase, mode: BuildMode) {
    let repo_root = std::env::current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mut cmd = Command::new(repo_root.join("target/debug/shipit"));
    cmd.current_dir(&repo_root)
        .arg(case.path)
        .arg("--skip-prepare")
        .arg("--start")
        .arg("--regenerate");

    match mode {
        BuildMode::Wasmer => {
            cmd.arg("--wasmer");
        }
        BuildMode::Docker => {
            cmd.arg("--wasmer").arg("--docker");
        }
        BuildMode::Local => {}
    }

    let port = if case.use_random_port {
        get_free_port()
    } else {
        8080
    };

    unsafe {
        std::env::set_var("PORT", port.to_string());
    }

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let output = Arc::new(Mutex::new(String::new()));
    let build_phrase = "Build complete ✅";
    let serve_re = Regex::new(case.serve_pattern).unwrap();

    let output_clone = Arc::clone(&output);
    let build_phrase_clone = build_phrase.to_string();

    let stdout_task = task::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let mut out = output_clone.lock().await;
            out.push_str(&format!("STDOUT: {}\n", line));
        }
    });

    let output_clone2 = Arc::clone(&output);
    let stderr_task = task::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let mut out = output_clone2.lock().await;
            out.push_str(&format!("STDERR: {}\n", line));
        }
    });

    // Wait for build and serve by polling the output
    let mut found_build = false;
    let mut found_serve = false;
    for _ in 0..1800 {
        // 180 seconds * 10
        let out = output.lock().await.clone();
        if !found_build && out.contains(&build_phrase_clone) {
            found_build = true;
        }
        if !found_serve && serve_re.is_match(&out) {
            found_serve = true;
        }
        if found_build && found_serve {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    assert!(found_build, "Build not found in output");
    assert!(found_serve, "Serve not found in output");

    // Now check HTTP
    if !case.http.is_empty() {
        let client = Client::new();
        for req in case.http {
            let url = format!("http://localhost:{}{}", port, req.path);
            let resp = client.get(&url).send().await.unwrap();
            let body = resp.text().await.unwrap();
            assert!(
                Regex::new(req.body_match).unwrap().is_match(&body),
                "Pattern '{}' not found in body: {}",
                req.body_match,
                body
            );
        }
    }

    // Kill the process
    child.kill().await.unwrap();
    child.wait().await.unwrap();

    // Wait for readers
    let _ = tokio::try_join!(stdout_task, stderr_task);
}
