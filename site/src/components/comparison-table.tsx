import { Check, X } from "lucide-react";

type AnswerValue =
  | boolean
  | "when-indicated"
  | "via-starlark"
  | {
      count: string;
      label: string;
    };

const capabilities: Array<{
  area: string;
  anybuild: AnswerValue;
  railpack: AnswerValue;
  devbox: AnswerValue;
  buildpacks: AnswerValue;
}> = [
  {
    area: "Adapt the build to different artifacts",
    anybuild: true,
    railpack: false,
    devbox: false,
    buildpacks: false,
  },
  {
    area: "Uses your existing dependencies",
    anybuild: "when-indicated",
    railpack: true,
    devbox: true,
    buildpacks: true,
  },
  {
    area: "Deploy to multiple providers",
    anybuild: true,
    railpack: false,
    devbox: false,
    buildpacks: false,
  },
  {
    area: "Programmable build",
    anybuild: "via-starlark",
    railpack: false,
    devbox: false,
    buildpacks: false,
  },
  {
    area: "Optimal deployment size",
    anybuild: true,
    railpack: false,
    devbox: false,
    buildpacks: false,
  },
  {
    area: "Run migrations and jobs",
    anybuild: true,
    railpack: false,
    devbox: true,
    buildpacks: true,
  },
  {
    area: "Supports build cache",
    anybuild: true,
    railpack: true,
    devbox: true,
    buildpacks: true,
  },
  {
    area: "Frameworks supported",
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

  const qualifier =
    value === "when-indicated"
      ? "When indicated"
      : value === "via-starlark"
        ? "Via Starlark"
        : null;
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
      {isSupported ? <Check aria-hidden="true" /> : <X aria-hidden="true" />}
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
                <th scope="row">{capability.area}</th>
                {products.map((product) => (
                  <td
                    key={product.key}
                    className={product.key === "anybuild" ? "comparison__anybuild" : undefined}
                  >
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
