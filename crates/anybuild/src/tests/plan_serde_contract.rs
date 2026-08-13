use crate::plan::{snapshot, Package, RunStep, Serve, Step, UseStep};
use indexmap::IndexMap;

fn package(name: &str, version: Option<&str>) -> Package {
    Package {
        name: name.to_owned(),
        version: version.map(str::to_owned),
        architecture: None,
    }
}

#[test]
fn serde_json_matches_the_plan_contract() {
    let run = RunStep {
        command: "npm run build".to_owned(),
        inputs: Some(Vec::new()),
        outputs: None,
        group: None,
    };
    let mut commands = IndexMap::new();
    commands.insert("start".to_owned(), "node server.js".to_owned());
    let serve = Serve {
        name: "app".to_owned(),
        provider: "node".to_owned(),
        runtime_port: None,
        build: vec![
            Step::Run(run.clone()),
            Step::Use(UseStep {
                dependencies: vec![package("node", Some("22"))],
            }),
        ],
        deps: vec![package("node", Some("22")), package("bash", None)],
        commands,
        cwd: None,
        prepare: Some(vec![RunStep {
            command: "node prepare.js".to_owned(),
            inputs: None,
            outputs: None,
            group: None,
        }]),
        mounts: None,
        volumes: None,
        env: None,
        services: None,
    };

    assert_eq!(
        serde_json::to_string(&Step::Run(run)).unwrap(),
        r#"{"__type__":"RunStep","command":"npm run build","inputs":[]}"#
    );
    assert_eq!(
        serde_json::to_value(serve).unwrap(),
        serde_json::json!({
            "name": "app",
            "provider": "node",
            "build": [
                {
                    "__type__": "RunStep",
                    "command": "npm run build",
                    "inputs": [],
                },
                {
                    "__type__": "UseStep",
                    "dependencies": [{"name": "node", "version": "22"}],
                },
            ],
            "deps": ["node@22", "bash"],
            "commands": {"start": "node server.js"},
            "prepare": [
                {"__type__": "RunStep", "command": "node prepare.js"},
            ],
            "mounts": [],
            "volumes": [],
            "env": {},
            "services": [],
        })
    );
}

#[test]
fn snapshot_render_uses_one_space_indent() {
    let serve = Serve {
        name: "app".to_owned(),
        provider: "node".to_owned(),
        runtime_port: None,
        build: Vec::new(),
        deps: Vec::new(),
        commands: IndexMap::new(),
        cwd: None,
        prepare: None,
        mounts: None,
        volumes: None,
        env: None,
        services: None,
    };

    assert_eq!(
        snapshot::render(
            &serve,
            std::path::Path::new("/build"),
            std::path::Path::new("/anybuild"),
            std::path::Path::new("/workspace"),
        ),
        concat!(
            "{\n",
            " \"build\": [],\n",
            " \"commands\": {},\n",
            " \"deps\": [],\n",
            " \"env\": {},\n",
            " \"mounts\": [],\n",
            " \"name\": \"app\",\n",
            " \"prepare\": [],\n",
            " \"provider\": \"node\",\n",
            " \"services\": [],\n",
            " \"volumes\": []\n",
            "}\n",
        )
    );
}
