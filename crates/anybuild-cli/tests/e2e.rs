//! One `#[test]` per (example, build-mode) pair, ported from the pytest
//! parametrization in `tests/test_e2e.py`.
//!
//! Naming: `<suite>__<mode>__<example>`, where `<suite>` matches the pytest
//! `e2e_<suite>` marker and `<mode>` is local / wasmer / wasmer_and_docker.
//! CI slices with nextest filter expressions, e.g.
//!   -E 'test(/^php__/)'            # one suite
//!   -E 'test(/__wasmer__/)'        # one mode
//!
//! Combinations pytest collects but always runtime-skips (case not enabled
//! for the build mode; phpix memory checks outside Wasmer) are structural
//! and simply not generated. `e2e_test_list_matches_case_table` (the only
//! non-ignored test here) keeps this list in lockstep with the case table.

// The `<suite>__<mode>__<example>` convention needs double underscores,
// which the snake-case lint dislikes.
#![allow(non_snake_case)]

mod e2e_harness;

use e2e_harness::BuildMode::{Local, Wasmer, WasmerAndDocker};

macro_rules! e2e_tests {
    ($($name:ident => ($id:literal, $mode:expr);)*) => {
        $(
            #[test]
            #[ignore = "e2e: needs wasmer/docker; run via nextest --run-ignored all"]
            fn $name() {
                if let Err(err) = e2e_harness::run_case($id, $mode) {
                    panic!("{err:#}");
                }
            }
        )*

        /// Not ignored: cheap structural check that runs in plain
        /// `cargo test --workspace`. Ensures this file lists exactly one
        /// test per (case, structurally-enabled build mode), correctly
        /// named.
        #[test]
        fn e2e_test_list_matches_case_table() {
            let generated: Vec<(&str, &str, e2e_harness::BuildMode)> = vec![
                $( (stringify!($name), $id, $mode) ),*
            ];
            e2e_harness::verify_test_list(&generated);
        }
    };
}

e2e_tests! {
    // examples/cdn
    static__local__cdn => ("cdn", Local);
    static__wasmer__cdn => ("cdn", Wasmer);
    static__wasmer_and_docker__cdn => ("cdn", WasmerAndDocker);
    // examples/php-nobuild (phpinfo)
    php__local__php_nobuild => ("php_nobuild", Local);
    php__wasmer__php_nobuild => ("php_nobuild", Wasmer);
    php__wasmer_and_docker__php_nobuild => ("php_nobuild", WasmerAndDocker);
    // examples/php-static-mixed (static index plus executable PHP page)
    php__local__php_static_mixed => ("php_static_mixed", Local);
    php__wasmer__php_static_mixed => ("php_static_mixed", Wasmer);
    php__wasmer_and_docker__php_static_mixed => ("php_static_mixed", WasmerAndDocker);
    // examples/php-api
    php__local__php_api => ("php_api", Local);
    php__wasmer__php_api => ("php_api", Wasmer);
    php__wasmer_and_docker__php_api => ("php_api", WasmerAndDocker);
    // examples/php-wordpress
    php__local__php_wordpress0 => ("php_wordpress0", Local);
    php__wasmer__php_wordpress0 => ("php_wordpress0", Wasmer);
    php__wasmer_and_docker__php_wordpress0 => ("php_wordpress0", WasmerAndDocker);
    // wordpress-6.9.4.zip release archive (Wasmer only)
    php__wasmer__wordpress_6_9_4 => ("wordpress_6_9_4", Wasmer);
    // examples/php-wordpress-empty (Wasmer only)
    php__wasmer__php_wordpress_empty => ("php_wordpress_empty", Wasmer);
    // bro-barbershop theme archive (Wasmer only)
    php__wasmer__wordpress_bro_barbershop_theme => ("wordpress_bro_barbershop_theme", Wasmer);
    // examples/php-wordpress phpix memory cap (Wasmer only)
    php__wasmer__php_wordpress1 => ("php_wordpress1", Wasmer);
    // examples/php-nobuild phpix no memory cap (Wasmer only)
    php__wasmer__php_nobuild_phpix => ("php_nobuild_phpix", Wasmer);
    // examples/static-nobuild
    static__local__static_nobuild => ("static_nobuild", Local);
    static__wasmer__static_nobuild => ("static_nobuild", Wasmer);
    static__wasmer_and_docker__static_nobuild => ("static_nobuild", WasmerAndDocker);
    // examples/static-htmlwithjs (Wasmer only)
    static__wasmer__static_htmlwithjs => ("static_htmlwithjs", Wasmer);
    // examples/staticfile
    static__local__staticfile => ("staticfile", Local);
    static__wasmer__staticfile => ("staticfile", Wasmer);
    static__wasmer_and_docker__staticfile => ("staticfile", WasmerAndDocker);
    // examples/staticfile-redirects (Wasmer only)
    static__wasmer__staticfile_redirects => ("staticfile_redirects", Wasmer);
    // examples/node
    node__local__node => ("node", Local);
    node__wasmer__node => ("node", Wasmer);
    // examples/node-hono
    node__local__node_hono => ("node_hono", Local);
    node__wasmer__node_hono => ("node_hono", Wasmer);
    // examples/node-fastify
    node__local__node_fastify => ("node_fastify", Local);
    node__wasmer__node_fastify => ("node_fastify", Wasmer);
    // examples/node-express (Wasmer only)
    node__wasmer__node_express => ("node_express", Wasmer);
    // examples/node-koa (Wasmer only)
    node__wasmer__node_koa => ("node_koa", Wasmer);
    // examples/node-h3 (Wasmer only)
    node__wasmer__node_h3 => ("node_h3", Wasmer);
    // examples/node-elysia (Wasmer only)
    node__wasmer__node_elysia => ("node_elysia", Wasmer);
    // examples/node-nestjs (Wasmer only)
    node__wasmer__node_nestjs => ("node_nestjs", Wasmer);
    // examples/node-nitro (Wasmer only)
    node__wasmer__node_nitro => ("node_nitro", Wasmer);
    // examples/node-tanstack-start-nitro (Wasmer only)
    node__wasmer__node_tanstack_start_nitro => ("node_tanstack_start_nitro", Wasmer);
    // examples/node-hydrogen (Wasmer only)
    node__wasmer__node_hydrogen => ("node_hydrogen", Wasmer);
    // examples/node-react-router (Wasmer only)
    node__wasmer__node_react_router => ("node_react_router", Wasmer);
    // examples/node-remix (Wasmer only)
    node__wasmer__node_remix => ("node_remix", Wasmer);
    // examples/node-solidstart (Wasmer only)
    node__wasmer__node_solidstart => ("node_solidstart", Wasmer);
    // examples/node-tanstack-start (Wasmer only)
    node__wasmer__node_tanstack_start => ("node_tanstack_start", Wasmer);
    // examples/node-xmcp (Wasmer only)
    node__wasmer__node_xmcp => ("node_xmcp", Wasmer);
    // examples/node-mastra (Wasmer only)
    node__wasmer__node_mastra => ("node_mastra", Wasmer);
    // examples/node-next
    node__local__node_next => ("node_next", Local);
    node__wasmer__node_next => ("node_next", Wasmer);
    // examples/node-astro
    node__local__node_astro => ("node_astro", Local);
    node__wasmer__node_astro => ("node_astro", Wasmer);
    // examples/hugo
    static__local__hugo => ("hugo", Local);
    static__wasmer__hugo => ("hugo", Wasmer);
    static__wasmer_and_docker__hugo => ("hugo", WasmerAndDocker);
    // examples/mkdocs
    staticpython__local__mkdocs => ("mkdocs", Local);
    staticpython__wasmer__mkdocs => ("mkdocs", Wasmer);
    staticpython__wasmer_and_docker__mkdocs => ("mkdocs", WasmerAndDocker);
    // examples/mkdocs-with-plugins
    staticpython__local__mkdocs_with_plugins => ("mkdocs_with_plugins", Local);
    staticpython__wasmer__mkdocs_with_plugins => ("mkdocs_with_plugins", Wasmer);
    staticpython__wasmer_and_docker__mkdocs_with_plugins => ("mkdocs_with_plugins", WasmerAndDocker);
    // examples/nodestatic-astro (Wasmer only)
    staticnode1__wasmer__nodestatic_astro => ("nodestatic_astro", Wasmer);
    // examples/nodestatic-next (Wasmer only)
    staticnode1__wasmer__nodestatic_next => ("nodestatic_next", Wasmer);
    // examples/nodestatic-nuxt (Wasmer only)
    staticnode1__wasmer__nodestatic_nuxt => ("nodestatic_nuxt", Wasmer);
    // examples/nodestatic-docusaurus (Wasmer only)
    staticnode1__wasmer__nodestatic_docusaurus => ("nodestatic_docusaurus", Wasmer);
    // examples/nodestatic-svelte (Wasmer only)
    staticnode1__wasmer__nodestatic_svelte => ("nodestatic_svelte", Wasmer);
    // examples/nodestatic-sveltekit (Wasmer only)
    staticnode1__wasmer__nodestatic_sveltekit => ("nodestatic_sveltekit", Wasmer);
    // examples/nodestatic-remix (Wasmer only)
    staticnode1__wasmer__nodestatic_remix => ("nodestatic_remix", Wasmer);
    // examples/nodestatic-eleventy
    staticnode1__local__nodestatic_eleventy => ("nodestatic_eleventy", Local);
    staticnode1__wasmer__nodestatic_eleventy => ("nodestatic_eleventy", Wasmer);
    // examples/nodestatic-vitepress
    staticnode1__local__nodestatic_vitepress => ("nodestatic_vitepress", Local);
    staticnode1__wasmer__nodestatic_vitepress => ("nodestatic_vitepress", Wasmer);
    // examples/nodestatic-vuepress
    staticnode1__local__nodestatic_vuepress => ("nodestatic_vuepress", Local);
    staticnode1__wasmer__nodestatic_vuepress => ("nodestatic_vuepress", Wasmer);
    // examples/nodestatic-hexo
    staticnode1__local__nodestatic_hexo => ("nodestatic_hexo", Local);
    staticnode1__wasmer__nodestatic_hexo => ("nodestatic_hexo", Wasmer);
    // examples/nodestatic-metalsmith
    staticnode1__local__nodestatic_metalsmith => ("nodestatic_metalsmith", Local);
    staticnode1__wasmer__nodestatic_metalsmith => ("nodestatic_metalsmith", Wasmer);
    // examples/nodestatic-assemble
    staticnode1__local__nodestatic_assemble => ("nodestatic_assemble", Local);
    staticnode1__wasmer__nodestatic_assemble => ("nodestatic_assemble", Wasmer);
    // examples/nodestatic-harp
    staticnode1__local__nodestatic_harp => ("nodestatic_harp", Local);
    staticnode1__wasmer__nodestatic_harp => ("nodestatic_harp", Wasmer);
    // examples/nodestatic-angular (Wasmer only)
    staticnode1__wasmer__nodestatic_angular => ("nodestatic_angular", Wasmer);
    // examples/nodestatic-brunch (Wasmer only)
    staticnode1__wasmer__nodestatic_brunch => ("nodestatic_brunch", Wasmer);
    // examples/nodestatic-create-react-app (Wasmer only)
    staticnode2__wasmer__nodestatic_create_react_app => ("nodestatic_create_react_app", Wasmer);
    // examples/nodestatic-docusaurus-old (Wasmer only)
    staticnode2__wasmer__nodestatic_docusaurus_old => ("nodestatic_docusaurus_old", Wasmer);
    // examples/nodestatic-ember (Wasmer only)
    staticnode2__wasmer__nodestatic_ember => ("nodestatic_ember", Wasmer);
    // examples/nodestatic-ionic-angular (Wasmer only)
    staticnode2__wasmer__nodestatic_ionic_angular => ("nodestatic_ionic_angular", Wasmer);
    // examples/nodestatic-ionic-react (Wasmer only)
    staticnode2__wasmer__nodestatic_ionic_react => ("nodestatic_ionic_react", Wasmer);
    // examples/nodestatic-parcel (Wasmer only)
    staticnode2__wasmer__nodestatic_parcel => ("nodestatic_parcel", Wasmer);
    // examples/nodestatic-polymer (Wasmer only)
    staticnode2__wasmer__nodestatic_polymer => ("nodestatic_polymer", Wasmer);
    // examples/nodestatic-preact (Wasmer only)
    staticnode2__wasmer__nodestatic_preact => ("nodestatic_preact", Wasmer);
    // examples/nodestatic-stencil (Wasmer only)
    staticnode2__wasmer__nodestatic_stencil => ("nodestatic_stencil", Wasmer);
    // examples/nodestatic-umijs (Wasmer only)
    staticnode2__wasmer__nodestatic_umijs => ("nodestatic_umijs", Wasmer);
    // examples/nodestatic-vite (Wasmer only)
    staticnode2__wasmer__nodestatic_vite => ("nodestatic_vite", Wasmer);
    // examples/nodestatic-vite-react (Wasmer only)
    staticnode2__wasmer__nodestatic_vite_react => ("nodestatic_vite_react", Wasmer);
    // examples/nodestatic-vue (Wasmer only)
    staticnode2__wasmer__nodestatic_vue => ("nodestatic_vue", Wasmer);
    // examples/nodestatic-sanity (Wasmer only)
    staticnode2__wasmer__nodestatic_sanity => ("nodestatic_sanity", Wasmer);
    // examples/nodestatic-storybook (Wasmer only)
    staticnode2__wasmer__nodestatic_storybook => ("nodestatic_storybook", Wasmer);
    // examples/python-fastapi
    python__local__python_fastapi => ("python_fastapi", Local);
    python__wasmer__python_fastapi => ("python_fastapi", Wasmer);
    python__wasmer_and_docker__python_fastapi => ("python_fastapi", WasmerAndDocker);
    // examples/python-flask
    python__local__python_flask => ("python_flask", Local);
    python__wasmer__python_flask => ("python_flask", Wasmer);
    python__wasmer_and_docker__python_flask => ("python_flask", WasmerAndDocker);
    // examples/python-django
    python__local__python_django => ("python_django", Local);
    python__wasmer__python_django => ("python_django", Wasmer);
    python__wasmer_and_docker__python_django => ("python_django", WasmerAndDocker);
    // examples/python-ffmpeg
    python__local__python_ffmpeg => ("python_ffmpeg", Local);
    python__wasmer__python_ffmpeg => ("python_ffmpeg", Wasmer);
    python__wasmer_and_docker__python_ffmpeg => ("python_ffmpeg", WasmerAndDocker);
    // examples/python-pillow
    python__local__python_pillow => ("python_pillow", Local);
    python__wasmer__python_pillow => ("python_pillow", Wasmer);
    python__wasmer_and_docker__python_pillow => ("python_pillow", WasmerAndDocker);
    // examples/python-pandoc
    python__local__python_pandoc => ("python_pandoc", Local);
    python__wasmer__python_pandoc => ("python_pandoc", Wasmer);
    python__wasmer_and_docker__python_pandoc => ("python_pandoc", WasmerAndDocker);
    // examples/python-procfile
    python__local__python_procfile => ("python_procfile", Local);
    python__wasmer__python_procfile => ("python_procfile", Wasmer);
    python__wasmer_and_docker__python_procfile => ("python_procfile", WasmerAndDocker);
    // examples/python-streamlit
    python__local__python_streamlit => ("python_streamlit", Local);
    python__wasmer__python_streamlit => ("python_streamlit", Wasmer);
    python__wasmer_and_docker__python_streamlit => ("python_streamlit", WasmerAndDocker);
}
