import type { ReactNode } from "react";
import {
  BulletList,
  Callout,
  CodeBlock,
  DocLink,
  DocsTable,
  InlineCode,
  OrderedList,
  Paragraph,
} from "@/components/docs-primitives";

export type DocSection = {
  id: string;
  title: string;
  content: ReactNode;
};

export type DocPage = {
  slug: string;
  title: string;
  description: string;
  sections: DocSection[];
};

export type DocsNavGroup = {
  title: string;
  items: Array<{ title: string; slug: string }>;
};

const gettingStarted: DocPage = {
  slug: "",
  title: "Getting Started",
  description: "Install Anybuild, let it detect your project, and run the complete build pipeline.",
  sections: [
    {
      id: "overview",
      title: "Overview",
      content: (
        <>
          <Paragraph>
            Anybuild detects a project, generates an editable <InlineCode>Anybuild</InlineCode>{" "}
            definition, evaluates it into a typed plan, builds the required artifacts, and can start
            the application. The default command runs the combined <InlineCode>auto</InlineCode>{" "}
            pipeline.
          </Paragraph>
          <CodeBlock>{`anybuild . --start`}</CodeBlock>
          <Paragraph>
            The first argument is the workspace path. A path with no explicit subcommand is treated
            as <InlineCode>anybuild auto</InlineCode>, so the compact command above is equivalent to{" "}
            <InlineCode>anybuild auto . --start</InlineCode>.
          </Paragraph>
        </>
      ),
    },
    {
      id: "install",
      title: "Install",
      content: (
        <>
          <Paragraph>Install the released CLI with the shell installer or Cargo.</Paragraph>
          <CodeBlock>{`curl -fsSL https://anybuild.run/install | sh`}</CodeBlock>
          <CodeBlock>{`cargo install anybuild-cli`}</CodeBlock>
          <Paragraph>
            Confirm the binary is available with <InlineCode>anybuild --version</InlineCode>.
          </Paragraph>
        </>
      ),
    },
    {
      id: "choose-an-environment",
      title: "Choose an environment",
      content: (
        <>
          <DocsTable
            headers={["Command", "Build environment", "Runtime"]}
            rows={[
              [<InlineCode>anybuild . --start</InlineCode>, "Local toolchain", "Local process"],
              [<InlineCode>anybuild . --docker --start</InlineCode>, "Docker", "Local process"],
              [<InlineCode>anybuild . --wasmer --start</InlineCode>, "Local toolchain", "Wasmer"],
              [<InlineCode>anybuild . --docker --wasmer --start</InlineCode>, "Docker", "Wasmer"],
            ]}
          />
          <Callout title="Build and runtime are independent">
            <InlineCode>--docker</InlineCode> selects the build backend.{" "}
            <InlineCode>--wasmer</InlineCode> selects the runtime and packaging runner. They can be
            combined.
          </Callout>
        </>
      ),
    },
    {
      id: "generated-files",
      title: "Generated files",
      content: (
        <>
          <BulletList>
            <li>
              <InlineCode>Anybuild</InlineCode> is the generated Starlark definition. It is yours to
              edit.
            </li>
            <li>
              <InlineCode>.anybuild/</InlineCode> contains build state, artifacts, volumes, and
              runner-specific files.
            </li>
            <li>
              In a subdirectory project, the definition is named{" "}
              <InlineCode>Anybuild.&lt;subdir-slug&gt;</InlineCode>.
            </li>
          </BulletList>
          <Paragraph>
            Legacy <InlineCode>Shipit</InlineCode> files and <InlineCode>.shipit</InlineCode> state
            are renamed automatically when the modern names are absent.
          </Paragraph>
        </>
      ),
    },
  ],
};

const installation: DocPage = {
  slug: "installation",
  title: "Installation",
  description: "Install the Anybuild CLI and the optional execution tools you intend to use.",
  sections: [
    {
      id: "shell-installer",
      title: "Shell installer",
      content: (
        <>
          <Paragraph>On macOS or Linux, use the Anybuild installer.</Paragraph>
          <CodeBlock>{`curl -fsSL https://anybuild.run/install | sh`}</CodeBlock>
        </>
      ),
    },
    {
      id: "cargo",
      title: "Cargo",
      content: (
        <>
          <Paragraph>The published Cargo package is named anybuild-cli.</Paragraph>
          <CodeBlock>{`cargo install anybuild-cli`}</CodeBlock>
          <Paragraph>
            The package installs the <InlineCode>anybuild</InlineCode> binary and retains the{" "}
            <InlineCode>shipit</InlineCode> binary alias for CLI compatibility.
          </Paragraph>
        </>
      ),
    },
    {
      id: "optional-tools",
      title: "Optional tools",
      content: (
        <DocsTable
          headers={["Feature", "Requirement"]}
          rows={[
            ["Local builds", "The language and package-manager tools required by the project."],
            [
              <InlineCode>--docker</InlineCode>,
              "Docker, Podman, Depot, or another compatible client selected with --docker-client.",
            ],
            [
              <InlineCode>--wasmer</InlineCode>,
              "The Wasmer CLI on PATH, or a custom binary passed with --wasmer-bin.",
            ],
          ]}
        />
      ),
    },
    {
      id: "verify",
      title: "Verify the installation",
      content: <CodeBlock>{`anybuild --version\nanybuild --help`}</CodeBlock>,
    },
  ],
};

const help: DocPage = {
  slug: "help",
  title: "Help and Troubleshooting",
  description:
    "Inspect detection and plans, isolate generation problems, and report useful details.",
  sections: [
    {
      id: "inspect",
      title: "Inspect before building",
      content: (
        <>
          <Paragraph>
            Start with the evaluated provider and non-default configuration. This does not execute
            build steps.
          </Paragraph>
          <CodeBlock>{`anybuild plan .
anybuild plan . --out plan.json`}</CodeBlock>
        </>
      ),
    },
    {
      id: "regenerate",
      title: "Check generated definitions",
      content: (
        <>
          <Paragraph>
            Regenerate when provider behavior has changed. Use a temporary definition to test fresh
            detection without replacing the checked-in <InlineCode>Anybuild</InlineCode> file.
          </Paragraph>
          <CodeBlock>{`anybuild . --regenerate --start
anybuild . --temp-anybuild --start`}</CodeBlock>
        </>
      ),
    },
    {
      id: "environment",
      title: "Check environment overrides",
      content: (
        <Paragraph>
          An <InlineCode>ANYBUILD_*</InlineCode> value overrides its{" "}
          <InlineCode>SHIPIT_*</InlineCode> compatibility fallback. Also check{" "}
          <InlineCode>PORT</InlineCode>, <InlineCode>--serve-port</InlineCode>, and the selected{" "}
          <InlineCode>.env</InlineCode> files when runtime behavior differs between environments.
        </Paragraph>
      ),
    },
    {
      id: "report",
      title: "Report a problem",
      content: (
        <>
          <Paragraph>
            Include the Anybuild version, command, selected provider, project manifest files, and
            the complete error with credentials removed.
          </Paragraph>
          <CodeBlock>{`anybuild --version
anybuild plan .`}</CodeBlock>
          <Paragraph>
            Search or open an issue in the{" "}
            <DocLink href="https://github.com/wasmerio/anybuild/issues">
              Anybuild issue tracker
            </DocLink>
            .
          </Paragraph>
        </>
      ),
    },
  ],
};

const howItWorks: DocPage = {
  slug: "how-it-works",
  title: "How Anybuild Works",
  description: "From source detection to a local process, Docker build, or Wasmer package.",
  sections: [
    {
      id: "pipeline",
      title: "The pipeline",
      content: (
        <OrderedList>
          <li>Resolve the workspace root and optional application subdirectory.</li>
          <li>
            Generate Anybuild file.
            <BulletList>
              <li>Read command hints from a Procfile and CLI overrides.</li>
              <li>Score every provider and select the highest-scoring match.</li>
              <li>Load typed provider configuration and apply environment and JSON overrides.</li>
            </BulletList>
          </li>
          <li>Load the Anybuild Starlark definition file.</li>
          <li>Evaluate the definition into a Serve plan.</li>
          <li>Execute build steps with a Builder backend (Local or Docker).</li>
          <li>Prepare and run a Runner backend (Local or Wasmer).</li>
        </OrderedList>
      ),
    },
    {
      id: "provider-selection",
      title: "Provider selection",
      content: (
        <>
          <Paragraph>
            Anybuild evaluates providers in specificity order and chooses the highest detection
            score. Laravel, Hugo, MkDocs, and other specific providers can therefore win over a
            generic PHP, Python, Node.js, or static-file match.
          </Paragraph>
          <CodeBlock>{`anybuild . --provider node --start`}</CodeBlock>
          <Paragraph>
            Use <InlineCode>--provider</InlineCode> when automatic detection is not the behavior you
            want.
          </Paragraph>
        </>
      ),
    },
    {
      id: "plan",
      title: "The evaluated plan",
      content: (
        <>
          <Paragraph>
            The Starlark evaluator produces one <InlineCode>Serve</InlineCode> value containing
            ordered build steps, runtime packages, named commands, mounts, volumes, environment
            variables, prepare steps, and backing services.
          </Paragraph>
          <CodeBlock>{`anybuild plan .\nanybuild plan . --out plan.json`}</CodeBlock>
        </>
      ),
    },
    {
      id: "state",
      title: "State and artifacts",
      content: (
        <Paragraph>
          Operation state is scoped to <InlineCode>.anybuild</InlineCode>. Subdirectory apps get
          isolated state beneath a normalized subdirectory slug, so multiple apps in one workspace
          do not overwrite each other.
        </Paragraph>
      ),
    },
  ],
};

const localDevelopment: DocPage = {
  slug: "guides/local-development",
  title: "Developing Locally",
  description: "Build, start, and invoke named project commands from your workstation.",
  sections: [
    {
      id: "one-command",
      title: "Build and start",
      content: (
        <>
          <CodeBlock>{`anybuild . --start`}</CodeBlock>
          <Paragraph>
            This generates the definition when missing, builds the project, and invokes the plan's{" "}
            <InlineCode>start</InlineCode> command.
          </Paragraph>
        </>
      ),
    },
    {
      id: "separate-phases",
      title: "Run phases separately",
      content: <CodeBlock>{`anybuild build .\nanybuild run . --start`}</CodeBlock>,
    },
    {
      id: "named-commands",
      title: "Named commands",
      content: (
        <>
          <Paragraph>
            A Serve plan can define commands such as <InlineCode>start</InlineCode>,{" "}
            <InlineCode>after_deploy</InlineCode>, or project-specific names.
          </Paragraph>
          <CodeBlock>{`anybuild run . --command prepare-db\nanybuild run . -c warm-cache --start`}</CodeBlock>
        </>
      ),
    },
    {
      id: "ports-and-volumes",
      title: "Ports and volumes",
      content: (
        <>
          <CodeBlock>{`anybuild . --start --serve-port 3000\nanybuild run . --start --volume uploads:/app/uploads`}</CodeBlock>
          <Paragraph>
            The run command receives <InlineCode>PORT</InlineCode>. If{" "}
            <InlineCode>--serve-port</InlineCode> is absent, Anybuild uses the process{" "}
            <InlineCode>PORT</InlineCode> and then defaults to 8080.
          </Paragraph>
        </>
      ),
    },
  ],
};

const buildEnvironments: DocPage = {
  slug: "guides/build-environments",
  title: "Build Environments",
  description: "Choose where build steps execute and where the resulting application runs.",
  sections: [
    {
      id: "local",
      title: "Local builder",
      content: (
        <Paragraph>
          Local mode executes plan steps against the host toolchain and writes artifacts under{" "}
          <InlineCode>.anybuild/local</InlineCode>. It is the default and the shortest feedback
          loop.
        </Paragraph>
      ),
    },
    {
      id: "docker",
      title: "Docker builder",
      content: (
        <>
          <CodeBlock>{`anybuild build . --docker\nanybuild build . --docker-client podman`}</CodeBlock>
          <Paragraph>
            Docker mode synthesizes a build container from the same plan, executes it with the
            selected Docker-compatible client, and exports the resulting artifacts back into
            Anybuild state.
          </Paragraph>
          <Callout title="Docker is a build sandbox">
            The Docker backend does not change the runtime by itself. Add{" "}
            <InlineCode>--wasmer</InlineCode> when the exported application should be packaged and
            run with Wasmer.
          </Callout>
        </>
      ),
    },
    {
      id: "wasmer",
      title: "Wasmer runtime",
      content: (
        <>
          <CodeBlock>{`anybuild . --wasmer --start\nanybuild . --docker --wasmer --start`}</CodeBlock>
          <Paragraph>
            Wasmer maps plan dependencies to WebAssembly packages, rewrites supported executable
            names, prepares package metadata, and runs the configured command through Wasmer.
          </Paragraph>
        </>
      ),
    },
  ],
};

const additionalPackages: DocPage = {
  slug: "guides/additional-packages",
  title: "Installing Additional Packages",
  description: "Add project dependencies, build tools, or runtime packages at the correct layer.",
  sections: [
    {
      id: "project-dependencies",
      title: "Prefer project manifests",
      content: (
        <Paragraph>
          Application libraries belong in the project's normal manifest—such as{" "}
          <InlineCode>package.json</InlineCode>, <InlineCode>pyproject.toml</InlineCode>,{" "}
          <InlineCode>requirements.txt</InlineCode>, <InlineCode>composer.json</InlineCode>, or{" "}
          <InlineCode>go.mod</InlineCode>. Anybuild's providers detect and install those manifests
          using the selected package manager.
        </Paragraph>
      ),
    },
    {
      id: "provider-extras",
      title: "Provider-specific extras",
      content: (
        <>
          <Paragraph>
            Providers that expose <InlineCode>extra_dependencies</InlineCode> accept a JSON array
            through the corresponding environment variable. Its meaning is provider-specific: Python
            adds Python packages with uv, while Node.js exposes additional Anybuild packages to
            build steps.
          </Paragraph>
          <CodeBlock>{`ANYBUILD_EXTRA_DEPENDENCIES='["orjson"]' anybuild . --provider python --start`}</CodeBlock>
        </>
      ),
    },
    {
      id: "build-and-runtime",
      title: "Build-time and runtime packages",
      content: (
        <>
          <Paragraph>
            In Starlark, <InlineCode>use(dep(...))</InlineCode> exposes a package to following build
            steps. <InlineCode>extra_deps</InlineCode> adds packages needed by the running
            application. Declare both when a tool is required in both phases.
          </Paragraph>
          <CodeBlock label="Anybuild">{`load("//anybuild/tools:node.bzl", "node_build", "node_serve")

build = node_build(config)

node_serve(
    config,
    build,
    build_pre = [use(dep("ffmpeg"))],
    extra_deps = [dep("ffmpeg")],
)`}</CodeBlock>
          <Callout title="Package availability depends on the environment">
            A package name and version must be resolvable by the selected local or Docker backend
            and by the selected runtime. Test the same build/runtime combination used in production.
          </Callout>
        </>
      ),
    },
  ],
};

const customSteps: DocPage = {
  slug: "guides/custom-steps",
  title: "Adding Build Steps",
  description: "Extend generated provider builds without replacing their detected defaults.",
  sections: [
    {
      id: "generated-file",
      title: "Start from the generated definition",
      content: (
        <>
          <CodeBlock>{`anybuild generate .`}</CodeBlock>
          <Paragraph>
            The generated file calls a provider build function and passes its build struct into a
            serve function. Keep that composition and add steps around it.
          </Paragraph>
        </>
      ),
    },
    {
      id: "before-and-after",
      title: "Run before or after the provider build",
      content: (
        <CodeBlock label="Anybuild">{`load("//anybuild/tools:node.bzl", "node_build", "node_serve")

build = node_build(config)

node_serve(
    config,
    build,
    build_pre = [run("node scripts/check.js", group = "check")],
    build_post = [run("node scripts/prerender.js", group = "build")],
)`}</CodeBlock>
      ),
    },
    {
      id: "step-builtins",
      title: "Step builtins",
      content: (
        <DocsTable
          headers={["Builtin", "Purpose"]}
          rows={[
            [<InlineCode>dep()</InlineCode>, "Declare a build or runtime package."],
            [<InlineCode>use()</InlineCode>, "Expose packages to later build steps."],
            [
              <InlineCode>run()</InlineCode>,
              "Execute a shell command with optional groups and I/O.",
            ],
            [<InlineCode>copy()</InlineCode>, "Copy project or embedded asset files."],
            [<InlineCode>workdir()</InlineCode>, "Change the build working directory."],
            [<InlineCode>env()</InlineCode>, "Set variables for following build steps."],
            [<InlineCode>write()</InlineCode>, "Create a file during the build."],
            [<InlineCode>path()</InlineCode>, "Prepend a directory to PATH."],
          ]}
        />
      ),
    },
    {
      id: "composition",
      title: "Compose providers",
      content: (
        <Paragraph>
          Provider build functions return ordinary structs. A documentation generator can use Python
          build steps and then pass the resulting static output into{" "}
          <InlineCode>staticfile_serve</InlineCode>; Laravel similarly combines PHP and Node.js
          work. Custom files can use the same pattern.
        </Paragraph>
      ),
    },
  ],
};

const production: DocPage = {
  slug: "guides/production",
  title: "Running in Production",
  description:
    "Make builds repeatable, choose the production runtime, and publish or integrate safely.",
  sections: [
    {
      id: "repeatable-definition",
      title: "Keep the definition repeatable",
      content: (
        <Paragraph>
          Generate the <InlineCode>Anybuild</InlineCode> definition, review it, and keep it with the
          project when reproducibility matters. Pin provider versions through configuration or
          environment overrides instead of depending indefinitely on changing defaults.
        </Paragraph>
      ),
    },
    {
      id: "validate",
      title: "Validate the production combination",
      content: (
        <>
          <Paragraph>
            Build and run with the same backend and runtime that production will use. Docker and
            Wasmer are independent selections.
          </Paragraph>
          <CodeBlock>{`anybuild plan . --docker --wasmer
anybuild . --docker --wasmer --start`}</CodeBlock>
        </>
      ),
    },
    {
      id: "publish",
      title: "Publish to Wasmer",
      content: (
        <>
          <CodeBlock>{`anybuild . --wasmer-deploy \\
  --wasmer-app-owner YOUR_OWNER \\
  --wasmer-app-name YOUR_APP`}</CodeBlock>
          <Paragraph>
            To let another system perform the final publication, write deployment configuration
            instead.
          </Paragraph>
          <CodeBlock>{`anybuild . --wasmer-deploy-config deploy.json`}</CodeBlock>
        </>
      ),
    },
    {
      id: "external-platforms",
      title: "Integrate another platform",
      content: (
        <Paragraph>
          A platform integration can call the CLI or the Rust SDK, inspect the structured{" "}
          <InlineCode>ProjectPlan</InlineCode>, and consume paths from{" "}
          <InlineCode>BuildOutcome</InlineCode>. The current Docker backend exports build artifacts;
          it does not produce or publish a general-purpose OCI application image.
        </Paragraph>
      ),
    },
    {
      id: "secrets",
      title: "Keep credentials outside the definition",
      content: (
        <Paragraph>
          Supply registry tokens and application secrets through the operation environment or
          platform secret store. SDK events redact values whose variable names indicate tokens,
          passwords, secrets, credentials, or API keys, but the values remain available to the child
          processes that need them.
        </Paragraph>
      ),
    },
  ],
};

const workspaces: DocPage = {
  slug: "guides/workspaces",
  title: "Workspaces and Subdirectories",
  description: "Build one application inside a larger repository without losing workspace context.",
  sections: [
    {
      id: "select-an-app",
      title: "Select an application",
      content: <CodeBlock>{`anybuild . --subdir apps/web --start`}</CodeBlock>,
    },
    {
      id: "definition-name",
      title: "Definition and state",
      content: (
        <>
          <Paragraph>
            A subdirectory such as <InlineCode>apps/web</InlineCode> generates{" "}
            <InlineCode>Anybuild.apps-web</InlineCode> at the workspace root. The file records{" "}
            <InlineCode>app_subdir = "apps/web"</InlineCode>, and state is isolated under{" "}
            <InlineCode>.anybuild/apps-web</InlineCode>.
          </Paragraph>
        </>
      ),
    },
    {
      id: "node-workspaces",
      title: "Node.js package managers",
      content: (
        <Paragraph>
          If a Node.js subdirectory has no lockfile of its own, Anybuild inherits the workspace
          root's detected package manager and rewrites package-manager-prefixed build commands when
          needed.
        </Paragraph>
      ),
    },
    {
      id: "path-safety",
      title: "Path safety",
      content: (
        <Paragraph>
          The subdirectory must be relative, must exist, must be a directory, and must remain inside
          the workspace root.
        </Paragraph>
      ),
    },
  ],
};

const anybuildFile: DocPage = {
  slug: "configuration/anybuild-file",
  title: "The Anybuild File",
  description: "The editable Starlark definition that connects provider detection to execution.",
  sections: [
    {
      id: "shape",
      title: "Basic shape",
      content: (
        <CodeBlock label="Anybuild">{`load("//anybuild/tools:python.bzl", "python_build", "python_serve")

build = python_build(config)

python_serve(config, build, name = "my-app")`}</CodeBlock>
      ),
    },
    {
      id: "build-and-serve",
      title: "Build and serve functions",
      content: (
        <>
          <Paragraph>
            Each provider exposes a build function that returns steps and runtime requirements, plus
            a serve function that assembles the final command, environment, mounts, volumes, and
            services.
          </Paragraph>
          <DocsTable
            headers={["Provider", "Build / serve"]}
            rows={[
              ["python", "python_build / python_serve"],
              ["node", "node_build / node_serve"],
              ["go", "go_build / go_serve"],
              ["php", "php_build / php_serve"],
              ["laravel", "laravel_build / laravel_serve"],
              ["wordpress", "wordpress_build / wordpress_serve"],
              ["staticfile", "staticfile_build / staticfile_serve"],
              ["hugo", "hugo_build / staticfile_serve"],
              ["jekyll", "jekyll_build / staticfile_serve"],
              ["mkdocs", "mkdocs_build / staticfile_serve"],
              ["node-static", "nodestatic_build / staticfile_serve"],
            ]}
          />
        </>
      ),
    },
    {
      id: "serve-overrides",
      title: "Serve overrides",
      content: (
        <Paragraph>
          All provider serve functions accept common overrides including{" "}
          <InlineCode>build_pre</InlineCode>, <InlineCode>build_post</InlineCode>,{" "}
          <InlineCode>extra_deps</InlineCode>, <InlineCode>extra_env</InlineCode>,{" "}
          <InlineCode>commands</InlineCode>, <InlineCode>prepare</InlineCode>,{" "}
          <InlineCode>cwd</InlineCode>, mounts, volumes, and services.
        </Paragraph>
      ),
    },
    {
      id: "loads",
      title: "Load labels",
      content: (
        <DocsTable
          headers={["Label", "Resolution"]}
          rows={[
            ["//anybuild/...:file.bzl", "Bundled Anybuild standard library."],
            ["//pkg/path:file.bzl", "A file under the project root."],
            ["file.bzl or ./file.bzl", "Relative to the file performing the load."],
          ]}
        />
      ),
    },
  ],
};

const environmentVariables: DocPage = {
  slug: "configuration/environment-variables",
  title: "Environment Variables",
  description: "Override provider configuration and layer runtime environment files predictably.",
  sections: [
    {
      id: "provider-overrides",
      title: "Provider overrides",
      content: (
        <>
          <Paragraph>
            Provider fields map to uppercase <InlineCode>ANYBUILD_*</InlineCode> names.
          </Paragraph>
          <CodeBlock>{`ANYBUILD_NODE_VERSION=22 anybuild . --start
ANYBUILD_PYTHON_VERSION=3.12 anybuild . --start
ANYBUILD_STATIC_DIR=dist anybuild . --start`}</CodeBlock>
          <Paragraph>
            The legacy <InlineCode>SHIPIT_*</InlineCode> form is consulted only when the matching{" "}
            <InlineCode>ANYBUILD_*</InlineCode> variable is absent.
          </Paragraph>
        </>
      ),
    },
    {
      id: "json-patch",
      title: "JSON patch",
      content: (
        <CodeBlock>{`anybuild build . --config '{"phpix":true,"php_version":"8.3.29"}'`}</CodeBlock>
      ),
    },
    {
      id: "dotenv",
      title: ".env layering",
      content: (
        <>
          <Paragraph>
            During build, Anybuild adds dotenv values to the serve environment in this order; later
            files override earlier values.
          </Paragraph>
          <OrderedList>
            <li>Environment declared by the evaluated Serve.</li>
            <li>Workspace root .env.</li>
            <li>Workspace root .env.&lt;name&gt; when --env-name is set.</li>
            <li>Application subdirectory .env.</li>
            <li>Application subdirectory .env.&lt;name&gt;.</li>
          </OrderedList>
        </>
      ),
    },
    {
      id: "port",
      title: "PORT",
      content: (
        <Paragraph>
          <InlineCode>--serve-port</InlineCode> takes precedence over process{" "}
          <InlineCode>PORT</InlineCode>. The default is 8080. References to{" "}
          <InlineCode>$PORT</InlineCode> in start and after-deploy commands are resolved for the
          active runner.
        </Paragraph>
      ),
    },
  ],
};

const commandsConfig: DocPage = {
  slug: "configuration/commands",
  title: "Commands and Procfiles",
  description: "Control install, build, start, and named lifecycle commands.",
  sections: [
    {
      id: "procfile",
      title: "Procfile discovery",
      content: (
        <>
          <Paragraph>
            Anybuild reads a root <InlineCode>Procfile</InlineCode>. The start command is selected
            from <InlineCode>web</InlineCode>, <InlineCode>default</InlineCode>, then{" "}
            <InlineCode>start</InlineCode>; a Procfile with one process also uses that sole value.
          </Paragraph>
          <CodeBlock label="Procfile">{`web: node server.js
after_deploy: node scripts/migrate.js`}</CodeBlock>
        </>
      ),
    },
    {
      id: "cli-overrides",
      title: "CLI overrides",
      content: (
        <CodeBlock>{`anybuild . \\
  --install-command "pnpm install --frozen-lockfile" \\
  --build-command "pnpm build" \\
  --start-command "node dist/server.js" \\
  --start`}</CodeBlock>
      ),
    },
    {
      id: "groups",
      title: "Build groups",
      content: (
        <Paragraph>
          Provider steps tag important commands with groups such as <InlineCode>install</InlineCode>
          , <InlineCode>build</InlineCode>, and <InlineCode>prune</InlineCode>. Command overrides
          replace the first matching grouped step while preserving the rest of the provider plan.
        </Paragraph>
      ),
    },
  ],
};

const excludingFiles: DocPage = {
  slug: "configuration/excluding-files",
  title: "Excluding Files",
  description: "Control which project files individual copy steps place into build artifacts.",
  sections: [
    {
      id: "provider-defaults",
      title: "Provider defaults",
      content: (
        <Paragraph>
          Built-in providers already avoid common development state. Node.js excludes{" "}
          <InlineCode>node_modules</InlineCode> and <InlineCode>.git</InlineCode>; Python excludes{" "}
          <InlineCode>.venv</InlineCode>, <InlineCode>__pycache__</InlineCode>, and{" "}
          <InlineCode>.git</InlineCode>. Other providers apply their own source-copy rules.
        </Paragraph>
      ),
    },
    {
      id: "copy-ignore",
      title: "Ignore patterns on copy steps",
      content: (
        <>
          <Paragraph>
            The <InlineCode>copy</InlineCode> builtin accepts an <InlineCode>ignore</InlineCode>{" "}
            list. Patterns apply to that copy step rather than to the whole plan.
          </Paragraph>
          <CodeBlock label="Anybuild">{`copy(
    ".",
    ".",
    ignore = [".git", "node_modules", "*.log", "coverage"],
)`}</CodeBlock>
          <Paragraph>
            Local builds match patterns against entry names recursively. Docker builds translate the
            list into copy exclusions. Prefer simple portable patterns when both backends must
            behave identically.
          </Paragraph>
        </>
      ),
    },
    {
      id: "no-global-file",
      title: "No global Anybuild ignore file",
      content: (
        <Paragraph>
          Anybuild does not currently read an <InlineCode>.anybuildignore</InlineCode> file. To
          change a provider's built-in source selection, edit the generated definition and compose
          lower-level provider helpers or replace the relevant copy step.
        </Paragraph>
      ),
    },
    {
      id: "internal-state",
      title: "Internal state is excluded",
      content: (
        <Paragraph>
          Anybuild excludes its own <InlineCode>.anybuild</InlineCode> state and definition from
          project-source copies. The Docker build context also excludes legacy{" "}
          <InlineCode>.shipit</InlineCode> and <InlineCode>Shipit</InlineCode> names.
        </Paragraph>
      ),
    },
  ],
};

const staticSites: DocPage = {
  slug: "providers/static-sites",
  title: "Static Sites",
  description: "Static HTML, Hugo, Jekyll, MkDocs, and Node-based site generators.",
  sections: [
    {
      id: "providers",
      title: "Static providers",
      content: (
        <DocsTable
          headers={["Provider", "Primary detection", "Default output"]}
          rows={[
            ["staticfile", "Staticfile, index.html, or public/index.html", "Configured root"],
            ["hugo", "hugo.toml/json/yaml/yml or a Hugo content layout", "public"],
            ["jekyll", "_config.yml or _config.yaml", "_site"],
            ["mkdocs", "mkdocs.yml or mkdocs.yaml", "site"],
            [
              "node-static",
              "A recognized static framework, export configuration, or static build command",
              "Framework-specific",
            ],
          ]}
        />
      ),
    },
    {
      id: "shared-runtime",
      title: "Shared static runtime",
      content: (
        <Paragraph>
          Generator-specific build functions feed their output into{" "}
          <InlineCode>staticfile_serve</InlineCode>. Static sites are served with the bundled
          static-web-server package, and <InlineCode>_redirects</InlineCode> can be translated into
          its redirect configuration.
        </Paragraph>
      ),
    },
    {
      id: "node-static-priority",
      title: "Node-static priority",
      content: (
        <Paragraph>
          Anybuild scores static Node.js output before the generic Node provider. A Vite, Astro,
          Docusaurus, Gatsby, or configured static export therefore does not need a Node start
          script—the built directory is served as static content.
        </Paragraph>
      ),
    },
    {
      id: "override-output",
      title: "Override the output directory",
      content: <CodeBlock>{`ANYBUILD_STATIC_DIR=dist anybuild . --start`}</CodeBlock>,
    },
  ],
};

const nodeProvider: DocPage = {
  slug: "providers/node",
  title: "Node.js",
  description: "Package-manager detection, framework-aware commands, and dependency optimization.",
  sections: [
    {
      id: "detection",
      title: "Detection",
      content: (
        <Paragraph>
          The Node provider recognizes Node start/install commands, package.json, known framework
          dependencies, and common JavaScript server entry files. Static-capable projects are
          evaluated by the higher-priority node-static provider first.
        </Paragraph>
      ),
    },
    {
      id: "package-manager",
      title: "Package manager",
      content: (
        <Paragraph>
          Anybuild reads the packageManager field first, then workspace and lockfile evidence for
          npm, pnpm, Yarn, or Bun. It stages lockfiles before installing so dependency work can be
          cached independently from source changes.
        </Paragraph>
      ),
    },
    {
      id: "commands",
      title: "Build and start commands",
      content: (
        <>
          <Paragraph>
            Build scripts and known framework commands determine the build step. Start inference
            considers package.json scripts, its main field, and common files such as server.js,
            app.js, index.js, src/server.js, and src/index.js.
          </Paragraph>
          <Callout title="Nitro projects">
            When Nitro is detected, Anybuild sets <InlineCode>NITRO_PRESET=node-server</InlineCode>{" "}
            for the build and starts <InlineCode>node .output/server/index.mjs</InlineCode>. Nitro
            and Astro outputs can use dependency tracing to reduce deployed node_modules.
          </Callout>
        </>
      ),
    },
    {
      id: "configuration",
      title: "Common configuration",
      content: (
        <DocsTable
          headers={["Variable", "Purpose"]}
          rows={[
            ["ANYBUILD_NODE_VERSION", "Node.js package version; current default is 24."],
            ["ANYBUILD_PACKAGE_MANAGER", "Force npm, pnpm, yarn, or bun."],
            ["ANYBUILD_BUILD_COMMAND", "Override the detected build command."],
            [
              "ANYBUILD_OPTIMIZE_NODE_DEPENDENCIES",
              "Enable dependency tracing for supported framework outputs.",
            ],
            [
              "ANYBUILD_REMOVE_NATIVE_BINARIES",
              "Remove executable native binaries from Edge-targeted dependencies.",
            ],
          ]}
        />
      ),
    },
  ],
};

const pythonProvider: DocPage = {
  slug: "providers/python",
  title: "Python",
  description: "Python, Django, FastAPI, Flask, ASGI, WSGI, and dependency-aware runtimes.",
  sections: [
    {
      id: "detection",
      title: "Detection",
      content: (
        <Paragraph>
          Python is detected from pyproject.toml, requirements.txt, manage.py, Python-oriented start
          commands, or a discovered Python main file. MkDocs is evaluated first and receives its own
          static-site provider.
        </Paragraph>
      ),
    },
    {
      id: "frameworks",
      title: "Frameworks and servers",
      content: (
        <Paragraph>
          Dependency and source inspection can identify Django, FastAPI, Flask, Streamlit, MCP,
          ASGI, and WSGI applications. Anybuild derives an application import and selects a
          compatible server such as uvicorn, gunicorn, hypercorn, daphne, or the framework CLI.
        </Paragraph>
      ),
    },
    {
      id: "dependencies",
      title: "Dependencies",
      content: (
        <Paragraph>
          Python installs are driven by uv and the discovered pyproject/requirements context.
          Referenced requirement and constraint files are included. Optional ffmpeg and pandoc
          dependencies become explicit plan packages when detected or configured.
        </Paragraph>
      ),
    },
    {
      id: "configuration",
      title: "Common configuration",
      content: (
        <DocsTable
          headers={["Variable", "Purpose"]}
          rows={[
            ["ANYBUILD_PYTHON_VERSION", "Python package version; current default is 3.13."],
            ["ANYBUILD_FRAMEWORK", "Force a supported framework."],
            ["ANYBUILD_SERVER", "Force a supported application server."],
            ["ANYBUILD_ASGI_APPLICATION", "Set the ASGI import path."],
            ["ANYBUILD_WSGI_APPLICATION", "Set the WSGI import path."],
            ["ANYBUILD_PRECOMPILE_PYTHON", "Control bytecode precompilation."],
            ["ANYBUILD_EXTRA_DEPENDENCIES", "JSON array of additional packages."],
          ]}
        />
      ),
    },
  ],
};

const phpProvider: DocPage = {
  slug: "providers/php",
  title: "PHP, Laravel, and WordPress",
  description: "Composer-aware PHP builds with specialized Laravel and WordPress providers.",
  sections: [
    {
      id: "selection",
      title: "Provider selection",
      content: (
        <Paragraph>
          Laravel is selected from artisan plus composer.json. WordPress recognizes full sites,
          plugin headers, theme headers, or an explicit WP version. Generic PHP recognizes composer
          projects, PHP entry files, and Drupal, Moodle, or Symfony layouts.
        </Paragraph>
      ),
    },
    {
      id: "php",
      title: "PHP",
      content: (
        <Paragraph>
          The PHP provider discovers the public directory, Composer usage and build scripts, and
          framework-specific document roots. Its current declared PHP default is 8.3.29.
        </Paragraph>
      ),
    },
    {
      id: "laravel",
      title: "Laravel",
      content: (
        <Paragraph>
          Laravel composes the PHP provider with Node.js asset steps. Composer is enabled, Node
          package-manager configuration is retained, and the generated Starlark definition combines
          both build graphs before serving the PHP application.
        </Paragraph>
      ),
    },
    {
      id: "wordpress",
      title: "WordPress",
      content: (
        <Paragraph>
          Full WordPress sites preserve their source layout. Standalone themes and plugins are
          detected from header metadata, receive a WordPress core version when needed, and record
          the activation target in provider configuration.
        </Paragraph>
      ),
    },
    {
      id: "configuration",
      title: "Common configuration",
      content: (
        <DocsTable
          headers={["Variable", "Purpose"]}
          rows={[
            ["ANYBUILD_PHP_VERSION", "Select the PHP package version."],
            ["ANYBUILD_USE_COMPOSER", "Control Composer installation."],
            ["ANYBUILD_PUBLIC_DIR", "Override the document root."],
            ["ANYBUILD_PHPIX", "Use the phpix runtime path."],
            ["ANYBUILD_WP_VERSION", "Select or force WordPress detection."],
            ["ANYBUILD_WP_LOCALE", "Set the WordPress locale."],
          ]}
        />
      ),
    },
  ],
};

const goProvider: DocPage = {
  slug: "providers/go",
  title: "Go",
  description: "Detect a Go module, compile its server entrypoint, and run the resulting binary.",
  sections: [
    {
      id: "detection",
      title: "Detection",
      content: (
        <Paragraph>
          A project is detected as Go when go.mod or go.sum exists. Anybuild searches common server
          files including main.go, server.go, serve.go, api.go, and web.go, including supported
          nested layouts.
        </Paragraph>
      ),
    },
    {
      id: "binary",
      title: "Build output",
      content: (
        <Paragraph>
          The selected source file determines a normalized serve binary name. If discovery cannot
          find an entrypoint, set <InlineCode>ANYBUILD_GO_BUILD_FILE</InlineCode> explicitly.
        </Paragraph>
      ),
    },
    {
      id: "configuration",
      title: "Configuration",
      content: (
        <DocsTable
          headers={["Variable", "Purpose"]}
          rows={[
            ["ANYBUILD_GO_VERSION", "Go package version; current default is 1.25.5."],
            ["ANYBUILD_GO_BUILD_FILE", "The Go server entry file to compile."],
            ["ANYBUILD_SERVE_BINARY", "The output binary name to run."],
          ]}
        />
      ),
    },
  ],
};

const localDeploy: DocPage = {
  slug: "deploying/local",
  title: "Local Preview",
  description: "Run the built artifact on your system with the plan's start command.",
  sections: [
    {
      id: "preview",
      title: "Build and preview",
      content: <CodeBlock>{`anybuild . --start`}</CodeBlock>,
    },
    {
      id: "existing-build",
      title: "Run an existing build",
      content: <CodeBlock>{`anybuild build .\nanybuild run . --start`}</CodeBlock>,
    },
    {
      id: "behavior",
      title: "Runtime behavior",
      content: (
        <Paragraph>
          The local runner loads the evaluated command and environment from Anybuild state, merges
          volume mappings, supplies PORT, and inherits subprocess I/O for an interactive terminal.
        </Paragraph>
      ),
    },
  ],
};

const wasmerDeploy: DocPage = {
  slug: "deploying/wasmer",
  title: "Deploying to Wasmer",
  description:
    "Package the evaluated application for Wasmer and publish or write deployment config.",
  sections: [
    {
      id: "build-and-run",
      title: "Build and run",
      content: <CodeBlock>{`anybuild . --wasmer --start`}</CodeBlock>,
    },
    {
      id: "publish",
      title: "Publish",
      content: (
        <>
          <CodeBlock>{`anybuild . --wasmer-deploy`}</CodeBlock>
          <Paragraph>
            Auto mode enables the Wasmer runtime, builds the package, and publishes it. Use{" "}
            <InlineCode>--wasmer-app-owner</InlineCode> and{" "}
            <InlineCode>--wasmer-app-name</InlineCode> to set publication identity.
          </Paragraph>
        </>
      ),
    },
    {
      id: "write-config",
      title: "Write deployment config",
      content: <CodeBlock>{`anybuild . --wasmer-deploy-config deploy.json`}</CodeBlock>,
    },
    {
      id: "connection",
      title: "Wasmer connection",
      content: (
        <Paragraph>
          Select a custom binary, registry, or token with <InlineCode>--wasmer-bin</InlineCode>,{" "}
          <InlineCode>--wasmer-registry</InlineCode>, and <InlineCode>--wasmer-token</InlineCode>.
          Tokens are passed to child operations but redacted from SDK events.
        </Paragraph>
      ),
    },
  ],
};

const externalPlatforms: DocPage = {
  slug: "deploying/external-platforms",
  title: "External Platforms",
  description: "Use Anybuild artifacts with container and provider-specific deployment workflows.",
  sections: [
    {
      id: "current-scope",
      title: "Current CLI scope",
      content: (
        <Callout title="Wasmer is the integrated publisher">
          The current Rust CLI's deploy operation publishes Wasmer packages or writes Wasmer
          deployment configuration. Docker is a build backend that exports artifacts; it is not a
          generic OCI-image deployment command.
        </Callout>
      ),
    },
    {
      id: "docker-builds",
      title: "Docker-isolated builds",
      content: (
        <CodeBlock>{`anybuild build . --docker\nanybuild build . --docker-client depot`}</CodeBlock>
      ),
    },
    {
      id: "platform-integration",
      title: "Platform integration",
      content: (
        <Paragraph>
          Platforms can invoke the CLI or the Rust SDK, inspect <InlineCode>ProjectPlan</InlineCode>
          , and consume the artifact paths returned by <InlineCode>BuildOutcome</InlineCode>.
          Provider-specific publishing remains the responsibility of that integration unless it
          targets Wasmer.
        </Paragraph>
      ),
    },
  ],
};

const cliReference: DocPage = {
  slug: "reference/cli",
  title: "CLI Reference",
  description: "Commands and important options exposed by the anybuild binary.",
  sections: [
    {
      id: "commands",
      title: "Commands",
      content: (
        <DocsTable
          headers={["Command", "Purpose"]}
          rows={[
            ["auto / implicit", "Generate when needed, build, optionally run and deploy."],
            ["generate", "Create or refresh an Anybuild definition."],
            ["plan", "Evaluate provider config, commands, and services without building."],
            ["build", "Execute build steps and prepare runtime artifacts."],
            ["run", "Invoke named commands from an existing build."],
            ["deploy", "Publish a built Wasmer package or write its deployment config."],
          ]}
        />
      ),
    },
    {
      id: "project-options",
      title: "Project and configuration options",
      content: (
        <DocsTable
          headers={["Option", "Meaning"]}
          rows={[
            ["PATH", "Project path; defaults to the current directory."],
            ["--subdir", "Application directory relative to the project root."],
            ["--anybuild-path", "Use a non-default Anybuild definition."],
            ["--provider", "Force a registered provider."],
            ["--config", "Merge a JSON object over detected provider configuration."],
            ["--install-command", "Override the first install-group step."],
            ["--build-command", "Override the first build-group step."],
            ["--start-command", "Override the start command."],
            ["--serve-port", "Set the plan and runtime port."],
          ]}
        />
      ),
    },
    {
      id: "execution-options",
      title: "Execution options",
      content: (
        <DocsTable
          headers={["Option", "Meaning"]}
          rows={[
            ["--docker", "Use the Docker build backend."],
            ["--docker-client", "Select a Docker-compatible client."],
            ["--docker-opts", "Pass extra client options."],
            ["--wasmer", "Use the Wasmer runtime and package layout."],
            ["--wasmer-bin", "Select the Wasmer binary."],
            ["--wasmer-registry", "Select the Wasmer registry."],
            ["--wasmer-token", "Authenticate Wasmer build/deploy operations."],
            ["--skip-prepare", "Do not execute plan prepare steps."],
            ["--env-name", "Load .env.<name> files during build."],
          ]}
        />
      ),
    },
    {
      id: "run-and-deploy",
      title: "Run and deploy options",
      content: (
        <DocsTable
          headers={["Option", "Meaning"]}
          rows={[
            ["-c, --command", "Run a named command; repeatable."],
            ["--start", "Run the start command."],
            ["--after-deploy", "Run the after_deploy command."],
            ["--volume NAME:/path", "Attach or override a named volume mapping."],
            ["--wasmer-deploy", "Build for Wasmer and publish."],
            ["--wasmer-deploy-config PATH", "Write deployment metadata instead of publishing."],
            ["--wasmer-app-owner", "Set the Wasmer owner."],
            ["--wasmer-app-name", "Set the Wasmer application name."],
          ]}
        />
      ),
    },
    {
      id: "generation-options",
      title: "Generation options",
      content: (
        <DocsTable
          headers={["Option", "Meaning"]}
          rows={[
            ["--regenerate", "Rewrite the detected Anybuild definition before auto/plan."],
            ["--temp-anybuild", "Use an operation-scoped temporary definition."],
            ["--out, -o", "Write generated definition or plan output to a path."],
          ]}
        />
      ),
    },
  ],
};

const sdkReference: DocPage = {
  slug: "reference/sdk",
  title: "Rust SDK",
  description: "Use the same detection and orchestration pipeline without spawning the CLI.",
  sections: [
    {
      id: "facade",
      title: "Project facade",
      content: (
        <CodeBlock label="Rust">{`use anybuild::{Anybuild, BuildOptions, RunOptions};

let project = Anybuild::new(".")
    .with_subdir("apps/web")
    .with_env("ANYBUILD_NODE_VERSION", "22");

let plan = project.plan(Default::default())?;
let build = project.build(BuildOptions::default())?;
let run = project.run(RunOptions::default().start())?;

# Ok::<(), anybuild::Error>(())`}</CodeBlock>
      ),
    },
    {
      id: "operations",
      title: "Operations and outcomes",
      content: (
        <DocsTable
          headers={["Method", "Outcome"]}
          rows={[
            ["generate", "GeneratedAnybuild: path, content, provider, effective config."],
            ["plan", "ProjectPlan: provider, config, services, evaluated Serve."],
            ["build", "BuildOutcome: plan and state directory."],
            ["run", "RunOutcome: executed and skipped command names."],
            ["deploy", "DeployOutcome: published identity or written config path."],
            ["auto", "AutoOutcome combining optional generation, build, run, and deploy."],
          ]}
        />
      ),
    },
    {
      id: "typed-options",
      title: "Typed execution options",
      content: (
        <DocsTable
          headers={["Type", "Choices"]}
          rows={[
            [<InlineCode>BuildEnvironment</InlineCode>, "Local or Docker(DockerOptions)."],
            [<InlineCode>RuntimeEnvironment</InlineCode>, "Local or Wasmer(WasmerOptions)."],
            [<InlineCode>GenerationPolicy</InlineCode>, "IfMissing, Always, or Temporary."],
            [
              <InlineCode>DeployTarget</InlineCode>,
              "Publish { owner, name } or WriteConfig { path }.",
            ],
            [
              <InlineCode>ProcessIo</InlineCode>,
              "Inherit subprocess streams or emit them as Events.",
            ],
          ]}
        />
      ),
    },
    {
      id: "environment",
      title: "Environment isolation",
      content: (
        <Paragraph>
          <InlineCode>Anybuild::new</InlineCode> snapshots the process environment. Per-instance{" "}
          <InlineCode>with_env</InlineCode> values override that snapshot, and{" "}
          <InlineCode>inherit_process_env(false)</InlineCode> creates a deterministic empty base
          environment without mutating global process state.
        </Paragraph>
      ),
    },
    {
      id: "events-and-io",
      title: "Events and process I/O",
      content: (
        <Paragraph>
          The SDK is silent for Anybuild-generated output until an{" "}
          <InlineCode>EventHandler</InlineCode> is installed.{" "}
          <InlineCode>ProcessIo::Inherit</InlineCode> keeps subprocesses interactive;{" "}
          <InlineCode>ProcessIo::Events</InlineCode> captures stdout and stderr as redacted
          structured events.
        </Paragraph>
      ),
    },
    {
      id: "errors",
      title: "Errors",
      content: (
        <Paragraph>
          <InlineCode>anybuild::Error</InlineCode> exposes an <InlineCode>ErrorKind</InlineCode>,
          operation name, project path, and underlying source. Error kinds are non-exhaustive so
          callers can handle broad categories safely.
        </Paragraph>
      ),
    },
  ],
};

const planReference: DocPage = {
  slug: "reference/plan",
  title: "Plan Model",
  description: "The stable public types produced by evaluating an Anybuild definition.",
  sections: [
    {
      id: "project-plan",
      title: "ProjectPlan",
      content: (
        <Paragraph>
          A ProjectPlan contains the selected provider, non-default effective configuration, backing
          services, and the full evaluated Serve value. The types required by consumers are exported
          under <InlineCode>anybuild::plan</InlineCode>.
        </Paragraph>
      ),
    },
    {
      id: "serve",
      title: "Serve",
      content: (
        <DocsTable
          headers={["Field", "Meaning"]}
          rows={[
            ["name / provider", "Deployment identity and selected provider label."],
            ["build", "Ordered Step values."],
            ["deps", "Runtime packages and optional versions/architectures."],
            ["commands", "Named runtime commands."],
            ["cwd / env", "Runtime working directory and environment."],
            ["prepare", "One-time commands after build and before serving."],
            ["mounts / volumes", "Artifact and persistent filesystem mappings."],
            ["services", "Postgres, MySQL, or Redis backing services."],
          ]}
        />
      ),
    },
    {
      id: "steps",
      title: "Steps",
      content: (
        <Paragraph>
          Public step variants are Run, Copy, Env, Path, Use, Workdir, and WriteFile. Run steps
          carry optional input/output hints and a group name; Copy steps distinguish project source
          from bundled runtime assets.
        </Paragraph>
      ),
    },
  ],
};

const designGoals: DocPage = {
  slug: "architecture/design-goals",
  title: "Design Goals",
  description: "The constraints that shape Anybuild's CLI, SDK, providers, and plans.",
  sections: [
    {
      id: "one-pipeline",
      title: "One project pipeline",
      content: (
        <Paragraph>
          Detection, generation, planning, building, running, and deployment are SDK operations. The
          CLI translates flags into those typed options and renders events; it does not own a second
          orchestration implementation.
        </Paragraph>
      ),
    },
    {
      id: "editable-defaults",
      title: "Automatic but editable",
      content: (
        <Paragraph>
          Provider detection should produce a useful default, but the generated Starlark file is a
          durable project artifact rather than an opaque internal plan. Users can compose or replace
          provider behavior.
        </Paragraph>
      ),
    },
    {
      id: "independent-environments",
      title: "Independent build and runtime choices",
      content: (
        <Paragraph>
          Build isolation and runtime packaging solve different problems. Anybuild models them
          independently as Local or Docker builds and Local or Wasmer runtimes.
        </Paragraph>
      ),
    },
    {
      id: "embeddable",
      title: "Embeddable and observable",
      content: (
        <Paragraph>
          The SDK is synchronous, silent by default, returns structured outcomes, emits owned
          events, redacts credentials, and captures the process environment per project instance.
        </Paragraph>
      ),
    },
    {
      id: "compatibility",
      title: "Compatibility without permanent duplication",
      content: (
        <Paragraph>
          Legacy Shipit files, state directories, environment variables, and the binary alias are
          migrated or accepted at boundaries. Internally, operations use Anybuild names and one Rust
          implementation.
        </Paragraph>
      ),
    },
  ],
};

const architecturePipeline: DocPage = {
  slug: "architecture/pipeline",
  title: "Pipeline Architecture",
  description: "How operation context, providers, Starlark, backends, and runners fit together.",
  sections: [
    {
      id: "operation-context",
      title: "Operation context",
      content: (
        <Paragraph>
          Every SDK call creates an operation-scoped context containing the effective environment,
          process-I/O policy, event reporter, and detected secret values. Backends and runners use
          this context instead of reading or mutating process-global state.
        </Paragraph>
      ),
    },
    {
      id: "providers",
      title: "Providers",
      content: (
        <Paragraph>
          Provider modules detect source layouts and load typed configuration. The registry
          currently contains Laravel, Hugo, MkDocs, Python, WordPress, PHP, node-static, Node.js,
          Jekyll, Go, and staticfile providers.
        </Paragraph>
      ),
    },
    {
      id: "evaluation",
      title: "Starlark evaluation",
      content: (
        <Paragraph>
          The evaluator loads bundled or project-local .bzl modules, exposes build builtins, and
          converts the final serve call into Rust plan types. Backend and runner path layouts are
          supplied during evaluation, so the same definition resolves correct artifact paths for
          each environment.
        </Paragraph>
      ),
    },
    {
      id: "execution",
      title: "Backends and runners",
      content: (
        <DocsTable
          headers={["Layer", "Implementations", "Responsibility"]}
          rows={[
            ["Build backend", "Local, Docker", "Execute steps and export artifacts."],
            ["Runner", "Local, Wasmer", "Prepare packages, map commands, run, and deploy."],
          ]}
        />
      ),
    },
    {
      id: "state-layout",
      title: "State layout",
      content: (
        <Paragraph>
          The workspace's <InlineCode>.anybuild</InlineCode> directory separates local, Docker,
          Wasmer, volume, and subdirectory state. Mounts have backend-specific build paths and
          runner-specific serve paths; Wasmer maps the app to /app and named mounts to /opt.
        </Paragraph>
      ),
    },
  ],
};

const railpackComparison: DocPage = {
  slug: "comparisons/railpack",
  title: "Anybuild vs Railpack",
  description:
    "Two automatic build systems with different outputs, execution models, and extension points.",
  sections: [
    {
      id: "summary",
      title: "Summary",
      content: (
        <Paragraph>
          Both tools detect application source and derive a build plan. Railpack is centered on
          generating and executing BuildKit plans for container images. Anybuild is centered on an
          editable Starlark project definition that can build locally or in Docker and run locally
          or as a Wasmer package.
        </Paragraph>
      ),
    },
    {
      id: "comparison",
      title: "Feature comparison",
      content: (
        <DocsTable
          headers={["Area", "Anybuild", "Railpack"]}
          rows={[
            [
              "Primary output",
              "Local artifacts and Wasmer packages; Docker can isolate builds.",
              "OCI images or exported filesystems produced through BuildKit.",
            ],
            [
              "Project configuration",
              "Generated, editable Starlark plus typed provider config.",
              "railpack.json patches over an automatically generated plan.",
            ],
            [
              "Execution",
              "Local or Docker build backend; local or Wasmer runner.",
              "BuildKit daemon/frontend and container runtime images.",
            ],
            [
              "Customization",
              "Compose provider build structs, steps, commands, mounts, volumes, and services.",
              "Patch steps, layers, caches, commands, and deploy inputs in JSON.",
            ],
            [
              "Documented integration surface",
              "Synchronous Rust SDK plus a thin CLI.",
              "CLI plus a BuildKit frontend for platform integration.",
            ],
            [
              "Integrated deployment",
              "Wasmer publishing and deployment-config output.",
              "Image production; a hosting platform performs deployment.",
            ],
          ]}
        />
      ),
    },
    {
      id: "choose-anybuild",
      title: "Choose Anybuild when",
      content: (
        <BulletList>
          <li>You need the same project plan to work on a host, in Docker, and with Wasmer.</li>
          <li>You want application-owned, programmable Starlark customization.</li>
          <li>You are embedding orchestration through a Rust API and structured events.</li>
          <li>You want an integrated path from detection to Wasmer publication.</li>
        </BulletList>
      ),
    },
    {
      id: "choose-railpack",
      title: "Choose Railpack when",
      content: (
        <BulletList>
          <li>Your required artifact is an OCI container image.</li>
          <li>Your platform already operates BuildKit and its cache/registry model.</li>
          <li>You prefer declarative JSON plan patches over a programmable project definition.</li>
          <li>You need Railpack's broader current language-provider catalog.</li>
        </BulletList>
      ),
    },
    {
      id: "sources",
      title: "Further reading",
      content: (
        <BulletList>
          <li>
            <DocLink href="https://railpack.com/getting-started">Railpack Getting Started</DocLink>
          </li>
          <li>
            <DocLink href="https://railpack.com/config/file">Railpack configuration model</DocLink>
          </li>
          <li>
            <DocLink href="https://railpack.com/reference/cli">
              Railpack CLI and BuildKit workflow
            </DocLink>
          </li>
        </BulletList>
      ),
    },
  ],
};

const buildpacksComparison: DocPage = {
  slug: "comparisons/buildpacks",
  title: "Anybuild vs Buildpacks",
  description: "Compare Anybuild's project pipeline with the Cloud Native Buildpacks standard.",
  sections: [
    {
      id: "summary",
      title: "Summary",
      content: (
        <Paragraph>
          Cloud Native Buildpacks are a standardized way to transform application source into OCI
          images through buildpacks, builders, and lifecycle phases. Anybuild is a project
          orchestration SDK and CLI whose providers produce a Serve plan for multiple build and
          runtime environments.
        </Paragraph>
      ),
    },
    {
      id: "comparison",
      title: "Feature comparison",
      content: (
        <DocsTable
          headers={["Area", "Anybuild", "Cloud Native Buildpacks"]}
          rows={[
            [
              "Detection unit",
              "A built-in scored provider selected once per project.",
              "One or more buildpacks selected by lifecycle detection and order groups.",
            ],
            [
              "Orchestrator",
              "Anybuild SDK operation and evaluated Serve plan.",
              "CNB lifecycle phases inside a builder/platform.",
            ],
            [
              "Output",
              "Backend artifacts and optional Wasmer package.",
              "A runnable OCI application image.",
            ],
            [
              "Customization",
              "Editable Starlark and provider composition.",
              "Buildpack APIs, project descriptors, bindings, and builder configuration.",
            ],
            [
              "Runtime",
              "Local process or Wasmer runner.",
              "Container runtime using the builder's referenced run image.",
            ],
            [
              "Standard lifecycle features",
              "No current rebase or SBOM contract in the public plan model.",
              "Standard analyze, detect, restore, build, export, rebase, and SBOM mechanisms.",
            ],
          ]}
        />
      ),
    },
    {
      id: "choose-anybuild",
      title: "Choose Anybuild when",
      content: (
        <BulletList>
          <li>You need a non-container local path or a Wasmer-native package.</li>
          <li>You want one editable project definition instead of authoring a buildpack.</li>
          <li>You need a small synchronous Rust SDK embedded in another tool.</li>
        </BulletList>
      ),
    },
    {
      id: "choose-buildpacks",
      title: "Choose Buildpacks when",
      content: (
        <BulletList>
          <li>OCI images are the required portable artifact.</li>
          <li>You need ecosystem-standard builders, lifecycle APIs, rebasing, or SBOM metadata.</li>
          <li>
            Your organization centralizes build policy in shared buildpacks and builder images.
          </li>
        </BulletList>
      ),
    },
    {
      id: "sources",
      title: "Further reading",
      content: (
        <BulletList>
          <li>
            <DocLink href="https://buildpacks.io/docs/for-app-developers/concepts/buildpack/">
              What is a buildpack?
            </DocLink>
          </li>
          <li>
            <DocLink href="https://buildpacks.io/docs/for-app-developers/concepts/builder/">
              What is a builder?
            </DocLink>
          </li>
          <li>
            <DocLink href="https://buildpacks.io/docs/for-platform-operators/concepts/lifecycle/">
              Buildpacks lifecycle phases
            </DocLink>
          </li>
        </BulletList>
      ),
    },
  ],
};

export const docsNav: DocsNavGroup[] = [
  {
    title: "Start here",
    items: [
      { title: "Getting Started", slug: "" },
      { title: "Installation", slug: "installation" },
      { title: "Help", slug: "help" },
      { title: "How Anybuild Works", slug: "how-it-works" },
    ],
  },
  {
    title: "Guides",
    items: [
      { title: "Developing Locally", slug: "guides/local-development" },
      { title: "Build Environments", slug: "guides/build-environments" },
      { title: "Installing Additional Packages", slug: "guides/additional-packages" },
      { title: "Adding Build Steps", slug: "guides/custom-steps" },
      { title: "Running in Production", slug: "guides/production" },
      { title: "Workspaces", slug: "guides/workspaces" },
    ],
  },
  {
    title: "Configuration",
    items: [
      { title: "The Anybuild File", slug: "configuration/anybuild-file" },
      { title: "Environment Variables", slug: "configuration/environment-variables" },
      { title: "Commands and Procfiles", slug: "configuration/commands" },
      { title: "Excluding Files", slug: "configuration/excluding-files" },
    ],
  },
  {
    title: "Providers",
    items: [
      { title: "Static Sites", slug: "providers/static-sites" },
      { title: "Node.js", slug: "providers/node" },
      { title: "Python", slug: "providers/python" },
      { title: "PHP and WordPress", slug: "providers/php" },
      { title: "Go", slug: "providers/go" },
    ],
  },
  {
    title: "Deploying",
    items: [
      { title: "Local Preview", slug: "deploying/local" },
      { title: "Wasmer", slug: "deploying/wasmer" },
      { title: "External Platforms", slug: "deploying/external-platforms" },
    ],
  },
  {
    title: "Reference",
    items: [
      { title: "CLI", slug: "reference/cli" },
      { title: "Rust SDK", slug: "reference/sdk" },
      { title: "Plan Model", slug: "reference/plan" },
    ],
  },
  {
    title: "Architecture",
    items: [
      { title: "Design Goals", slug: "architecture/design-goals" },
      { title: "Pipeline", slug: "architecture/pipeline" },
    ],
  },
  {
    title: "Comparisons",
    items: [
      { title: "Anybuild vs Railpack", slug: "comparisons/railpack" },
      { title: "Anybuild vs Buildpacks", slug: "comparisons/buildpacks" },
    ],
  },
];

export const docsPages: DocPage[] = [
  gettingStarted,
  installation,
  help,
  howItWorks,
  localDevelopment,
  buildEnvironments,
  additionalPackages,
  customSteps,
  production,
  workspaces,
  anybuildFile,
  environmentVariables,
  commandsConfig,
  excludingFiles,
  staticSites,
  nodeProvider,
  pythonProvider,
  phpProvider,
  goProvider,
  localDeploy,
  wasmerDeploy,
  externalPlatforms,
  cliReference,
  sdkReference,
  planReference,
  designGoals,
  architecturePipeline,
  railpackComparison,
  buildpacksComparison,
];

export function getDocPage(slug: string | undefined): DocPage | undefined {
  const normalized = (slug ?? "").replace(/^\/+|\/+$/g, "");
  return docsPages.find((page) => page.slug === normalized);
}

export function docHref(slug: string): string {
  return slug ? `/docs/${slug}` : "/docs";
}
