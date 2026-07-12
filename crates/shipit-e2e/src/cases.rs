//! The e2e case table, ported 1:1 from `tests/test_e2e.py`.
//!
//! Every field of the pytest `E2ECase` dataclass is preserved here. Cases
//! appear in the same order as the pytest parametrize list. `test_id` is the
//! pytest case id (name, else path, else download stem) with the
//! `examples/` prefix stripped and non-identifier characters mapped to `_`;
//! duplicate ids carry the same `0`/`1`/`2` suffixes pytest generates
//! (php_nobuild0/1/2, php_wordpress0/1).

use crate::BuildMode;

/// Suite assignment, mirroring `E2ETechnology` + `_technology_for_case`.
/// CI slices per suite with nextest filters like `test(/^php__/)`, matching
/// the pytest `e2e_<suite>` markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suite {
    Static,
    StaticPython,
    StaticNode1,
    StaticNode2,
    Python,
    Node,
    Php,
}

impl Suite {
    pub fn slug(self) -> &'static str {
        match self {
            Suite::Static => "static",
            Suite::StaticPython => "staticpython",
            Suite::StaticNode1 => "staticnode1",
            Suite::StaticNode2 => "staticnode2",
            Suite::Python => "python",
            Suite::Node => "node",
            Suite::Php => "php",
        }
    }
}

/// Port of the pytest `HTTPRequest` dataclass.
#[derive(Debug, Clone, Copy)]
pub struct HttpRequest {
    pub path: &'static str,
    pub body_match: Option<&'static str>,
    pub method: &'static str,
    pub expected_status: Option<u16>,
    pub location_match: Option<&'static str>,
    pub follow_redirects: bool,
}

impl HttpRequest {
    /// Port of `_http_readiness_request`: status-only probe, keeps method
    /// and redirect policy, defaults the expected status to 200.
    pub fn readiness(&self) -> HttpRequest {
        HttpRequest {
            path: self.path,
            body_match: None,
            method: self.method,
            expected_status: Some(self.expected_status.unwrap_or(200)),
            location_match: None,
            follow_redirects: self.follow_redirects,
        }
    }
}

const fn body(path: &'static str, body_match: &'static str) -> HttpRequest {
    HttpRequest {
        path,
        body_match: Some(body_match),
        method: "GET",
        expected_status: None,
        location_match: None,
        follow_redirects: true,
    }
}

const fn body_status(path: &'static str, status: u16, body_match: &'static str) -> HttpRequest {
    HttpRequest {
        path,
        body_match: Some(body_match),
        method: "GET",
        expected_status: Some(status),
        location_match: None,
        follow_redirects: true,
    }
}

const fn status(path: &'static str, status: u16) -> HttpRequest {
    HttpRequest {
        path,
        body_match: None,
        method: "GET",
        expected_status: Some(status),
        location_match: None,
        follow_redirects: true,
    }
}

const fn redirect(path: &'static str, status: u16, location_match: &'static str) -> HttpRequest {
    HttpRequest {
        path,
        body_match: None,
        method: "GET",
        expected_status: Some(status),
        location_match: Some(location_match),
        follow_redirects: false,
    }
}

/// Port of the pytest `RunCommand` dataclass.
#[derive(Debug, Clone, Copy)]
pub struct RunCommand {
    pub command: &'static str,
    pub stdout_match: Option<&'static str>,
    pub stderr_match: Option<&'static str>,
    pub expected_returncode: i32,
}

const fn run(command: &'static str) -> RunCommand {
    RunCommand {
        command,
        stdout_match: None,
        stderr_match: None,
        expected_returncode: 0,
    }
}

const fn run_stdout(command: &'static str, stdout_match: &'static str) -> RunCommand {
    RunCommand {
        command,
        stdout_match: Some(stdout_match),
        stderr_match: None,
        expected_returncode: 0,
    }
}

/// Port of the pytest `E2ECase` dataclass.
#[derive(Debug, Clone, Copy)]
pub struct Case {
    /// Sanitized pytest case id; unique across the table.
    pub test_id: &'static str,
    /// Suite (pytest `e2e_<suite>` marker); derived from path/name/download
    /// exactly like `_technology_for_case` (verified by a unit test).
    pub suite: Suite,
    pub path: Option<&'static str>,
    pub download: Option<&'static str>,
    pub name: Option<&'static str>,
    pub serve_pattern: &'static str,
    pub http: &'static [HttpRequest],
    pub use_random_port: bool,
    pub env: &'static [(&'static str, &'static str)],
    pub extra_env: &'static [(&'static str, &'static str)],
    pub create_db: bool,
    pub create_wp_content_volume: bool,
    pub run_after_deploy: bool,
    pub commands: &'static [RunCommand],
    pub expected_memory_limit: Option<&'static str>,
    pub expect_no_memory_limit: bool,
    pub build_modes: Option<&'static [BuildMode]>,
}

impl Case {
    /// The build modes for which a test structurally exists.
    ///
    /// In pytest every case is collected for all three modes and then
    /// runtime-skipped ("case is not enabled for this build mode" /
    /// "phpix memory-cap checks run in Wasmer mode only"). Both conditions
    /// are decidable from the case data alone, so here they determine which
    /// tests are generated at all.
    pub fn structural_modes(&self) -> Vec<BuildMode> {
        BuildMode::ALL
            .iter()
            .copied()
            .filter(|mode| {
                if (self.expected_memory_limit.is_some() || self.expect_no_memory_limit)
                    && *mode != BuildMode::Wasmer
                {
                    return false;
                }
                match self.build_modes {
                    Some(modes) => modes.contains(mode),
                    None => true,
                }
            })
            .collect()
    }
}

const BASE: Case = Case {
    test_id: "",
    suite: Suite::Static,
    path: None,
    download: None,
    name: None,
    serve_pattern: "",
    http: &[],
    use_random_port: true,
    env: &[],
    extra_env: &[],
    create_db: false,
    create_wp_content_volume: false,
    run_after_deploy: false,
    commands: &[],
    expected_memory_limit: None,
    expect_no_memory_limit: false,
    build_modes: None,
};

const PHP_DEV_SERVER: &str =
    r"PHP 8\.3\.[0-9]+ Development Server \(http://localhost:[\d]+\) started";
const SWS_LISTENING: &str = r"server is listening on";
const UVICORN: &str = r"Uvicorn running on .*";

const WASMER_ONLY: &[BuildMode] = &[BuildMode::Wasmer];
const LOCAL_ONLY: &[BuildMode] = &[BuildMode::Local];
const LOCAL_AND_WASMER: &[BuildMode] = &[BuildMode::Local, BuildMode::Wasmer];

const WORDPRESS_DB_ENV: &[(&str, &str)] = &[
    ("DB_NAME", "test"),
    ("DB_USERNAME", "root"),
    ("DB_HOST", "127.0.0.1"),
    ("DB_PORT", "3306"),
    ("DB_PASSWORD", ""),
    ("SHIPIT_PHPIX", "true"),
];

pub const BRO_BARBERSHOP_ARCHIVE_URL: &str =
    "https://github.com/motopress/bro-barbershop/archive/refs/heads/master.zip";

pub static CASES: &[Case] = &[
    // CDN/static fixture
    Case {
        test_id: "cdn",
        suite: Suite::Static,
        path: Some("examples/cdn"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"My CDN")],
        ..BASE
    },
    // Simple PHP site that calls phpinfo()
    Case {
        test_id: "php_nobuild0",
        suite: Suite::Php,
        path: Some("examples/php-nobuild"),
        serve_pattern: PHP_DEV_SERVER,
        http: &[body("/", r"PHP Version 8\.3\.[0-9]+")],
        ..BASE
    },
    // Simple PHP site that calls phpinfo() with no port
    Case {
        test_id: "php_nobuild1",
        suite: Suite::Php,
        path: Some("examples/php-nobuild"),
        serve_pattern: PHP_DEV_SERVER,
        http: &[body("/", r"PHP Version 8\.3\.[0-9]+")],
        ..BASE
    },
    // PHP API example with JSON at / and greeting endpoint
    Case {
        test_id: "php_api",
        suite: Suite::Php,
        path: Some("examples/php-api"),
        serve_pattern: PHP_DEV_SERVER,
        http: &[
            body("/", r#""version"\s*:\s*"8\.3\.[0-9]+""#),
            body("/api/greet/Alice", r"Hello, Alice!"),
        ],
        ..BASE
    },
    // WordPress skeleton that echoes a simple string
    Case {
        test_id: "php_wordpress0",
        suite: Suite::Php,
        path: Some("examples/php-wordpress"),
        serve_pattern: PHP_DEV_SERVER,
        http: &[body("/", r"WordPress")],
        ..BASE
    },
    // Full WordPress release archive, built and run through Wasmer only.
    Case {
        test_id: "wordpress_6_9_4",
        suite: Suite::Php,
        download: Some("https://wordpress.org/wordpress-6.9.4.zip"),
        serve_pattern: r"listening addr",
        http: &[body_status("/", 200, r"WordPress")],
        use_random_port: false,
        env: WORDPRESS_DB_ENV,
        create_db: true,
        create_wp_content_volume: true,
        run_after_deploy: true,
        commands: &[run_stdout(
            r#"wp eval 'echo json_encode(["status" => "ok"]);'"#,
            r#"\{"status":"ok"\}"#,
        )],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Full WordPress release archive, built and run through Wasmer only.
    Case {
        test_id: "php_wordpress_empty",
        suite: Suite::Php,
        path: Some("examples/php-wordpress-empty"),
        serve_pattern: r"listening addr",
        http: &[body_status("/", 200, r"WordPress")],
        use_random_port: false,
        env: &[
            ("DB_NAME", "test"),
            ("DB_USERNAME", "root"),
            ("DB_HOST", "127.0.0.1"),
            ("DB_PORT", "3306"),
            ("DB_PASSWORD", ""),
            ("SHIPIT_PHPIX", "true"),
            ("SHIPIT_WP_VERSION", "latest"),
            // ("SHIPIT_WP_LOCALE", "en_US"),
        ],
        create_db: true,
        create_wp_content_volume: true,
        run_after_deploy: true,
        commands: &[run_stdout(
            r#"wp eval 'echo json_encode(["status" => "ok"]);'"#,
            r#"\{"status":"ok"\}"#,
        )],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Real WordPress theme from GitHub; validates custom theme activation.
    Case {
        test_id: "wordpress_bro_barbershop_theme",
        suite: Suite::Php,
        download: Some(BRO_BARBERSHOP_ARCHIVE_URL),
        name: Some("wordpress_bro_barbershop_theme"),
        serve_pattern: r"listening addr",
        http: &[status("/", 200)],
        use_random_port: false,
        env: &[
            ("DB_NAME", "test"),
            ("DB_USERNAME", "root"),
            ("DB_HOST", "127.0.0.1"),
            ("DB_PORT", "3306"),
            ("DB_PASSWORD", ""),
            ("SHIPIT_PHPIX", "true"),
            ("SHIPIT_WP_VERSION", "6.9.4"),
        ],
        create_db: true,
        create_wp_content_volume: true,
        run_after_deploy: true,
        commands: &[
            run("wp theme is-active bro-barbershop"),
            run_stdout("wp option get stylesheet", r"^bro-barbershop\s*$"),
        ],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // WordPress skeleton in phpix mode (Wasmer only), validate memory cap.
    Case {
        test_id: "php_wordpress1",
        suite: Suite::Php,
        path: Some("examples/php-wordpress"),
        serve_pattern: PHP_DEV_SERVER,
        http: &[body("/", r"WordPress")],
        extra_env: &[("SHIPIT_PHPIX", "true")],
        expected_memory_limit: Some("2Gb"),
        ..BASE
    },
    // Non-WordPress phpix mode should not force a memory capability.
    Case {
        test_id: "php_nobuild2",
        suite: Suite::Php,
        path: Some("examples/php-nobuild"),
        serve_pattern: PHP_DEV_SERVER,
        http: &[body("/", r"PHP Version 8\.3\.[0-9]+")],
        extra_env: &[("SHIPIT_PHPIX", "true")],
        expect_no_memory_limit: true,
        ..BASE
    },
    // Static site copied as-is (no build step beyond copy)
    Case {
        test_id: "static_nobuild",
        suite: Suite::Static,
        path: Some("examples/static-nobuild"),
        // static-web-server banner varies; rely on HTTP check with generous
        // pattern
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Test")],
        ..BASE
    },
    // Static HTML app with browser JavaScript should not be treated as Node.
    Case {
        test_id: "static_htmlwithjs",
        suite: Suite::Static,
        path: Some("examples/static-htmlwithjs"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Static HTML with JS")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Staticfile provider serving content under site/
    Case {
        test_id: "staticfile",
        suite: Suite::Static,
        path: Some("examples/staticfile"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Hello from static site!")],
        ..BASE
    },
    // Staticfile provider redirect support via _redirects (Wasmer only).
    Case {
        test_id: "staticfile_redirects",
        suite: Suite::Static,
        path: Some("examples/staticfile-redirects"),
        serve_pattern: SWS_LISTENING,
        http: &[
            redirect("/docs/getting-started", 301, r"^/guides/getting-started/$"),
            body("/guides/getting-started/", r"Redirect target page"),
        ],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Generic Node HTTP server
    Case {
        test_id: "node",
        suite: Suite::Node,
        path: Some("examples/node"),
        serve_pattern: r"Node server listening on",
        http: &[body("/", r"Hello from Node")],
        build_modes: Some(LOCAL_AND_WASMER),
        ..BASE
    },
    // Hono app running on Node
    Case {
        test_id: "node_hono",
        suite: Suite::Node,
        path: Some("examples/node-hono"),
        serve_pattern: r"Hono server listening on",
        http: &[body("/", r"Hello from Hono on Shipit")],
        build_modes: Some(LOCAL_AND_WASMER),
        ..BASE
    },
    // Fastify app running on Node
    Case {
        test_id: "node_fastify",
        suite: Suite::Node,
        path: Some("examples/node-fastify"),
        serve_pattern: r"Fastify server listening on",
        http: &[body("/", r"Hello from Fastify on Shipit")],
        build_modes: Some(LOCAL_AND_WASMER),
        ..BASE
    },
    // Express app running on Node
    Case {
        test_id: "node_express",
        suite: Suite::Node,
        path: Some("examples/node-express"),
        serve_pattern: r"Express server listening on",
        http: &[body("/", r"Hello from Express on Shipit")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Koa app running on Node
    Case {
        test_id: "node_koa",
        suite: Suite::Node,
        path: Some("examples/node-koa"),
        serve_pattern: r"Koa server listening on",
        http: &[body("/", r"Hello from Koa on Shipit")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // H3 app running on Node
    Case {
        test_id: "node_h3",
        suite: Suite::Node,
        path: Some("examples/node-h3"),
        serve_pattern: r"H3 server listening on",
        http: &[body("/", r"Hello from H3 on Shipit")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Elysia app running on the Node adapter
    Case {
        test_id: "node_elysia",
        suite: Suite::Node,
        path: Some("examples/node-elysia"),
        serve_pattern: r"Elysia server listening on",
        http: &[body("/", r"Hello from Elysia on Shipit")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // NestJS-compatible Node runtime fixture
    Case {
        test_id: "node_nestjs",
        suite: Suite::Node,
        path: Some("examples/node-nestjs"),
        serve_pattern: r"NestJS server listening on",
        http: &[body("/", r"Hello from NestJS on Shipit")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Nitro-compatible Node runtime fixture
    Case {
        test_id: "node_nitro",
        suite: Suite::Node,
        path: Some("examples/node-nitro"),
        serve_pattern: r"Nitro server listening on",
        http: &[body("/", r"Hello from Nitro on Shipit")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Hydrogen-compatible Node runtime fixture
    Case {
        test_id: "node_hydrogen",
        suite: Suite::Node,
        path: Some("examples/node-hydrogen"),
        serve_pattern: r"Hydrogen server listening on",
        http: &[body("/", r"Hello from Hydrogen on Shipit")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // React Router runtime fixture with Vite-like dependencies
    Case {
        test_id: "node_react_router",
        suite: Suite::Node,
        path: Some("examples/node-react-router"),
        serve_pattern: r"React Router server listening on",
        http: &[body("/", r"Hello from React Router on Shipit")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Remix runtime fixture with server dependencies
    Case {
        test_id: "node_remix",
        suite: Suite::Node,
        path: Some("examples/node-remix"),
        serve_pattern: r"Remix server listening on",
        http: &[body("/", r"Hello from Remix on Shipit")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // SolidStart-compatible Node runtime fixture
    Case {
        test_id: "node_solidstart",
        suite: Suite::Node,
        path: Some("examples/node-solidstart"),
        serve_pattern: r"SolidStart server listening on",
        http: &[body("/", r"Hello from SolidStart on Shipit")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // TanStack Start-compatible Node runtime fixture
    Case {
        test_id: "node_tanstack_start",
        suite: Suite::Node,
        path: Some("examples/node-tanstack-start"),
        serve_pattern: r"TanStack Start server listening on",
        http: &[body("/", r"Hello from TanStack Start on Shipit")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // XMCP-compatible Node runtime fixture
    Case {
        test_id: "node_xmcp",
        suite: Suite::Node,
        path: Some("examples/node-xmcp"),
        serve_pattern: r"XMCP server listening on",
        http: &[body("/", r"Hello from XMCP on Shipit")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Mastra-compatible Node runtime fixture
    Case {
        test_id: "node_mastra",
        suite: Suite::Node,
        path: Some("examples/node-mastra"),
        serve_pattern: r"Mastra server listening on",
        http: &[body("/", r"Hello from Mastra on Shipit")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Next.js runtime app bundled for Node
    Case {
        test_id: "node_next",
        suite: Suite::Node,
        path: Some("examples/node-next"),
        serve_pattern: r"Next.js|started server|ready",
        http: &[body("/shipit-health.txt", r"Hello from Next\.js on Shipit")],
        build_modes: Some(LOCAL_AND_WASMER),
        ..BASE
    },
    // Astro runtime app served by the Node adapter
    Case {
        test_id: "node_astro",
        suite: Suite::Node,
        path: Some("examples/node-astro"),
        serve_pattern: r"Node|Astro|Listening|ready",
        http: &[body("/", r"Astro Node Example")],
        build_modes: Some(LOCAL_ONLY),
        ..BASE
    },
    // Hugo static site (built via Hugo, served with static-web-server)
    Case {
        test_id: "hugo",
        suite: Suite::Static,
        path: Some("examples/hugo"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"My New Hugo Site")],
        ..BASE
    },
    // MkDocs site (built with mkdocs, served with static-web-server)
    Case {
        test_id: "mkdocs",
        suite: Suite::StaticPython,
        path: Some("examples/mkdocs"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Welcome to MkDocs")],
        ..BASE
    },
    // MkDocs with plugins
    Case {
        test_id: "mkdocs_with_plugins",
        suite: Suite::StaticPython,
        path: Some("examples/mkdocs-with-plugins"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Welcome to MkDocs with Plugins")],
        ..BASE
    },
    // Astro static site
    Case {
        test_id: "nodestatic_astro",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-astro"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Astro Static Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Next.js static export via output: "export"
    Case {
        test_id: "nodestatic_next",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-next"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Get started by editing")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Nuxt static generation
    Case {
        test_id: "nodestatic_nuxt",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-nuxt"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Nuxt Static Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Docusaurus static documentation site
    Case {
        test_id: "nodestatic_docusaurus",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-docusaurus"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Docusaurus Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // SvelteKit prerendered static site
    Case {
        test_id: "nodestatic_svelte",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-svelte"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Svelte Static Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // SvelteKit static site
    Case {
        test_id: "nodestatic_sveltekit",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-sveltekit"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"SvelteKit Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Remix static output served as files
    Case {
        test_id: "nodestatic_remix",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-remix"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Remix Static Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Eleventy / 11ty static site
    Case {
        test_id: "nodestatic_eleventy",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-eleventy"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Eleventy Example")],
        build_modes: Some(LOCAL_AND_WASMER),
        ..BASE
    },
    // VitePress static documentation site
    Case {
        test_id: "nodestatic_vitepress",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-vitepress"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"VitePress Example")],
        build_modes: Some(LOCAL_AND_WASMER),
        ..BASE
    },
    // VuePress static documentation site
    Case {
        test_id: "nodestatic_vuepress",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-vuepress"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"VuePress Example")],
        build_modes: Some(LOCAL_AND_WASMER),
        ..BASE
    },
    // Hexo static blog
    Case {
        test_id: "nodestatic_hexo",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-hexo"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Hexo Example")],
        build_modes: Some(LOCAL_AND_WASMER),
        ..BASE
    },
    // Metalsmith static site
    Case {
        test_id: "nodestatic_metalsmith",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-metalsmith"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Metalsmith Example")],
        build_modes: Some(LOCAL_AND_WASMER),
        ..BASE
    },
    // Assemble static site
    Case {
        test_id: "nodestatic_assemble",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-assemble"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Assemble Example")],
        build_modes: Some(LOCAL_AND_WASMER),
        ..BASE
    },
    // Harp static site
    Case {
        test_id: "nodestatic_harp",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-harp"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Harp Example")],
        build_modes: Some(LOCAL_AND_WASMER),
        ..BASE
    },
    // Angular static app
    Case {
        test_id: "nodestatic_angular",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-angular"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Angular Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Brunch static app
    Case {
        test_id: "nodestatic_brunch",
        suite: Suite::StaticNode1,
        path: Some("examples/nodestatic-brunch"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Brunch Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Create React App static app
    Case {
        test_id: "nodestatic_create_react_app",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-create-react-app"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Create React App Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Docusaurus classic static app
    Case {
        test_id: "nodestatic_docusaurus_old",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-docusaurus-old"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Docusaurus Classic Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Ember static app
    Case {
        test_id: "nodestatic_ember",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-ember"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Ember Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Ionic Angular static app
    Case {
        test_id: "nodestatic_ionic_angular",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-ionic-angular"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Ionic Angular Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Ionic React static app
    Case {
        test_id: "nodestatic_ionic_react",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-ionic-react"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Ionic React Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Parcel static app
    Case {
        test_id: "nodestatic_parcel",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-parcel"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Parcel Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Polymer static app
    Case {
        test_id: "nodestatic_polymer",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-polymer"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Polymer Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Preact static app
    Case {
        test_id: "nodestatic_preact",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-preact"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Preact Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Stencil static app
    Case {
        test_id: "nodestatic_stencil",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-stencil"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Stencil Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // UmiJS static app
    Case {
        test_id: "nodestatic_umijs",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-umijs"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"UmiJS Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Vite static app
    Case {
        test_id: "nodestatic_vite",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-vite"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Vite Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Vite React static app
    Case {
        test_id: "nodestatic_vite_react",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-vite-react"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Vite React Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Vue CLI static app
    Case {
        test_id: "nodestatic_vue",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-vue"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Vue Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Sanity Studio static app
    Case {
        test_id: "nodestatic_sanity",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-sanity"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Sanity Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Storybook static app
    Case {
        test_id: "nodestatic_storybook",
        suite: Suite::StaticNode2,
        path: Some("examples/nodestatic-storybook"),
        serve_pattern: SWS_LISTENING,
        http: &[body("/", r"Storybook Example")],
        build_modes: Some(WASMER_ONLY),
        ..BASE
    },
    // Python FastAPI app on Uvicorn
    Case {
        test_id: "python_fastapi",
        suite: Suite::Python,
        path: Some("examples/python-fastapi"),
        serve_pattern: UVICORN,
        http: &[body("/", r"Hello World from fastapi!")],
        ..BASE
    },
    // Python Flask app served via Uvicorn WSGI
    Case {
        test_id: "python_flask",
        suite: Suite::Python,
        path: Some("examples/python-flask"),
        serve_pattern: UVICORN,
        http: &[body("/", r"Welcome to Flask")],
        ..BASE
    },
    // Python Django via Uvicorn WSGI. Docker covers collectstatic.
    Case {
        test_id: "python_django",
        suite: Suite::Python,
        path: Some("examples/python-django"),
        serve_pattern: UVICORN,
        http: &[body("/", r"Django")],
        build_modes: Some(&[
            BuildMode::Local,
            BuildMode::Wasmer,
            BuildMode::WasmerAndDocker,
        ]),
        ..BASE
    },
    // Python ffmpeg demo (FastAPI), homepage is static HTML form
    Case {
        test_id: "python_ffmpeg",
        suite: Suite::Python,
        path: Some("examples/python-ffmpeg"),
        serve_pattern: UVICORN,
        http: &[body("/", r"Take screenshot at 1s")],
        ..BASE
    },
    // Python Pillow demo (FastAPI), homepage has form title
    Case {
        test_id: "python_pillow",
        suite: Suite::Python,
        path: Some("examples/python-pillow"),
        serve_pattern: UVICORN,
        http: &[body("/", r"Image Crop\s*&\s*Rotate")],
        ..BASE
    },
    // Python Pandoc demo: app may require pandoc binary; only assert serve
    // started
    Case {
        test_id: "python_pandoc",
        suite: Suite::Python,
        path: Some("examples/python-pandoc"),
        serve_pattern: UVICORN,
        http: &[],
        ..BASE
    },
    // Python Procfile demo using python -m http.server
    Case {
        test_id: "python_procfile",
        suite: Suite::Python,
        path: Some("examples/python-procfile"),
        serve_pattern: r"Serving HTTP on .*",
        http: &[body("/", r"Test")],
        ..BASE
    },
    // Python Streamlit app
    Case {
        test_id: "python_streamlit",
        suite: Suite::Python,
        path: Some("examples/python-streamlit"),
        serve_pattern: r".*You can now view your Streamlit app in your browser.*",
        http: &[body("/", r"Streamlit")],
        ..BASE
    },
];
