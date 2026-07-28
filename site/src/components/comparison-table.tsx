import { Check, CircleHelp, X } from "lucide-react";

type AnswerValue =
  | boolean
  | "via-starlark"
  | {
      count: string;
      label: string;
    };

const capabilities: Array<{
  area: string;
  description: string;
  anybuild: AnswerValue;
  railpack: AnswerValue;
  devbox: AnswerValue;
  buildpacks: AnswerValue;
}> = [
  {
    area: "Adapt the build to different artifacts",
    description:
      "Builds can target different artifact formats and runtimes without changing the application.",
    anybuild: true,
    railpack: false,
    devbox: false,
    buildpacks: false,
  },
  {
    area: "Can deploy to Edge providers",
    description:
      "Deploys non-Docker artifacts directly to Edge providers without requiring a container image.",
    anybuild: true,
    railpack: false,
    devbox: false,
    buildpacks: false,
  },
  {
    area: "Can build with your local tools",
    description:
      "Runs builds directly with the toolchains already available on the developer's machine, without requiring a container.",
    anybuild: true,
    railpack: false,
    devbox: false,
    buildpacks: false,
  },
  {
    area: "Programmable build",
    description:
      "Build behavior can be expressed as code instead of being limited to fixed configuration.",
    anybuild: "via-starlark",
    railpack: false,
    devbox: false,
    buildpacks: false,
  },
  {
    area: "Optimal deployment size",
    description:
      "Deployment artifacts contain only the files and dependencies required to run the application.",
    anybuild: true,
    railpack: false,
    devbox: false,
    buildpacks: false,
  },
  {
    area: "Supports migrations and jobs",
    description:
      "Supports one-off commands such as database migrations, release tasks, and background jobs.",
    anybuild: true,
    railpack: false,
    devbox: true,
    buildpacks: true,
  },
  {
    area: "Supports build cache",
    description:
      "Reuses unchanged dependencies and build outputs to make subsequent builds faster.",
    anybuild: true,
    railpack: true,
    devbox: true,
    buildpacks: false,
  },
  {
    area: "Frameworks supported",
    description:
      "The number of frameworks, providers, templates, or official buildpacks supported by each tool.",
    anybuild: { count: "64", label: "64 frameworks supported" },
    railpack: { count: "12 / 24", label: "12 Railpack and 24 Nixpacks providers" },
    devbox: { count: "38+", label: "38 or more published Devbox templates" },
    buildpacks: { count: "13+", label: "13 or more official Heroku buildpacks" },
  },
];

const products = [
  { key: "anybuild", name: "Anybuild" },
  { key: "railpack", name: "Railpack / Nixpacks" },
  { key: "devbox", name: "Devbox" },
  { key: "buildpacks", name: "Heroku Buildpacks" },
] as const;

function Answer({ value }: { value: AnswerValue }) {
  if (typeof value === "object") {
    return (
      <span className="comparison__count" aria-label={value.label} title={value.label}>
        {value.count}
      </span>
    );
  }

  const qualifier = value === "via-starlark" ? "Via Starlark" : null;
  const isSupported = value !== false;
  const label = qualifier ?? (isSupported ? "Yes" : "No");

  return (
    <span
      className={`comparison__answer comparison__answer--${
        qualifier ? "conditional" : isSupported ? "yes" : "no"
      }`}
      role="img"
      aria-label={label}
      title={label}
    >
      <span className="comparison__answer-icon" aria-hidden="true">
        {isSupported ? <Check /> : <X />}
      </span>
      {qualifier ? <small>{qualifier}</small> : null}
    </span>
  );
}

export function ComparisonTable() {
  return (
    <section className="comparison" aria-labelledby="comparison-heading">
      <div className="comparison__intro">
        <h2 id="comparison-heading">How it compares</h2>
        <p>A direct capability comparison across today’s application build tools.</p>
      </div>

      <div className="comparison__shell">
        <table>
          <caption className="sr-only">
            Capability comparison of Anybuild, Railpack and Nixpacks, Devbox, and Heroku Buildpacks
          </caption>
          <thead>
            <tr>
              <th scope="col">Capability</th>
              {products.map((product) => (
                <th
                  key={product.key}
                  scope="col"
                  className={product.key === "anybuild" ? "comparison__anybuild" : undefined}
                >
                  <span>{product.name}</span>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {capabilities.map((capability) => (
              <tr key={capability.area}>
                <th scope="row">
                  <span className="comparison__capability">
                    <span>{capability.area}</span>
                    <span className="comparison__tooltip">
                      <button
                        type="button"
                        aria-label={`About ${capability.area}`}
                        aria-describedby={`comparison-tooltip-${capability.area
                          .toLowerCase()
                          .replaceAll(" ", "-")}`}
                      >
                        <CircleHelp aria-hidden="true" />
                      </button>
                      <span
                        id={`comparison-tooltip-${capability.area
                          .toLowerCase()
                          .replaceAll(" ", "-")}`}
                        className="comparison__tooltip-content"
                        role="tooltip"
                      >
                        {capability.description}
                      </span>
                    </span>
                  </span>
                </th>
                {products.map((product) => (
                  <td
                    key={product.key}
                    className={product.key === "anybuild" ? "comparison__anybuild" : undefined}
                  >
                    <span className="comparison__mobile-product" aria-hidden="true">
                      {product.name}
                    </span>
                    <Answer value={capability[product.key]} />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
