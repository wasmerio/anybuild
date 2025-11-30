use shipit::detect::detect_provider;
use shipit::generator::{GeneratorOptions, ShipitGenerator};
use shipit::model::{CustomCommands, ProviderPlan};
use shipit::procfile::Procfile;
use shipit::provider::registry;
use std::fs;
use std::path::Path;
use anyhow::Context as _;

use pretty_assertions::assert_eq;

macro_rules! generate_shipit_test {
    ($example:ident) => {
        paste::paste! {
            #[test]
            fn [<test_generate_ $example>]() {
                let example_name = stringify!($example);
                let example_path = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .join("examples")
                    .join(example_name.replace('_', "-"));

                if !example_path.is_dir() {
                    panic!("Example path '{}' does not exist or is not a directory", example_path.display());
                }

                let mut custom = CustomCommands::default();
                if example_path.join("Procfile").exists() {
                    let procfile_content = fs::read_to_string(example_path.join("Procfile")).unwrap();
                    let procfile = Procfile::parse(&procfile_content).unwrap();
                    if let Some(start_cmd) = procfile.start_command() {
                        custom.start = Some(start_cmd.to_string());
                    }
                }

                let registry = registry::providers();
                let (provider, _) = detect_provider(&registry, &example_path, &custom)
                    .unwrap()
                    .with_context(|| format!(
                        "Failed to detect provider for example '{}' at path '{}'", example_name, example_path.display()))
                    .unwrap();



                let plan = provider.plan().unwrap();

                let generator = ShipitGenerator::new(GeneratorOptions::default());
                let generated = generator.generate(&plan).unwrap();
                let expected = fs::read_to_string(example_path.join("Shipit")).unwrap();
                assert_eq!(generated.trim(), expected.trim());
            }
        }
    };
}

// List all examples with Shipit
generate_shipit_test!(php_nobuild);
generate_shipit_test!(php_api);
generate_shipit_test!(php_wordpress);
generate_shipit_test!(static_nobuild);
generate_shipit_test!(staticfile);
generate_shipit_test!(hugo);
generate_shipit_test!(mkdocs);
generate_shipit_test!(mkdocs_with_plugins);
generate_shipit_test!(python_fastapi);
generate_shipit_test!(python_flask);
generate_shipit_test!(python_django);
generate_shipit_test!(python_ffmpeg);
generate_shipit_test!(python_pillow);
generate_shipit_test!(python_procfile);
generate_shipit_test!(python_streamlit);
generate_shipit_test!(python_pandoc);
// Add more as needed
