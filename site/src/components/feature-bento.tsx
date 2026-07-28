import type { CSSProperties, ReactNode } from "react";
import {
  ArrowRight,
  Check,
  Container,
  Package,
  Search,
  Terminal,
  WandSparkles,
} from "lucide-react";

const frameworkLogos = [
  {
    name: "Next.js",
    src: "https://cdn.jsdelivr.net/gh/devicons/devicon@latest/icons/nextjs/nextjs-original.svg",
  },
  {
    name: "Astro",
    src: "https://astro.build/assets/press/astro-icon-light-gradient.svg",
  },
  {
    name: "Django",
    src: "/django-logo.svg",
  },
  {
    name: "WordPress",
    src: "https://upload.wikimedia.org/wikipedia/commons/9/98/WordPress_blue_logo.svg",
  },
  {
    name: "Laravel",
    src: "https://cdn.jsdelivr.net/gh/devicons/devicon@latest/icons/laravel/laravel-original.svg",
  },
  {
    name: "Hugo",
    src: "https://cdn.jsdelivr.net/gh/devicons/devicon@latest/icons/hugo/hugo-original.svg",
  },
];

function FeatureCopy({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="feature-bento__copy">
      <h3>{title}</h3>
      <p>{children}</p>
    </div>
  );
}

function ZeroConfigVisual() {
  return (
    <div className="feature-terminal" aria-hidden="true">
      <div className="feature-terminal__layer feature-terminal__layer--back" />
      <div className="feature-terminal__layer feature-terminal__layer--middle" />
      <div className="feature-terminal__window">
        <div className="feature-terminal__bar">
          <span />
          <span />
          <span />
          <small>anybuild</small>
        </div>
        <div className="feature-terminal__body">
          <div className="feature-terminal__command">
            <span>$</span> anybuild
            <i />
          </div>
          <div className="feature-terminal__step feature-terminal__step--one">
            <Search />
            Detected Astro project
          </div>
          <div className="feature-terminal__step feature-terminal__step--two">
            <Check />
            Generated build plan
          </div>
          <div className="feature-terminal__step feature-terminal__step--three">
            <Check />
            Ready to deploy
          </div>
        </div>
        <div className="feature-terminal__badge">
          <WandSparkles />
          Zero configuration
        </div>
      </div>
    </div>
  );
}

function WorkflowVisual() {
  const jobs = ["Detect provider", "Build artifact", "Prepare runtime"];
  return (
    <div className="feature-workflow" aria-hidden="true">
      <div className="feature-workflow__root">
        <Terminal />
        <span>anybuild</span>
        <small>1.8s</small>
      </div>
      <div className="feature-workflow__track">
        <i />
      </div>
      <div className="feature-workflow__jobs">
        {jobs.map((job, index) => (
          <div
            key={job}
            className="feature-workflow__job"
            style={{ "--job-index": index } as CSSProperties}
          >
            <span>
              <Check />
            </span>
            {job}
          </div>
        ))}
      </div>
    </div>
  );
}

function FrameworkVisual() {
  return (
    <div className="feature-frameworks" aria-hidden="true">
      <div className="feature-frameworks__grid">
        {frameworkLogos.map((framework, index) => (
          <div
            key={framework.name}
            className="feature-frameworks__logo"
            style={{ "--logo-index": index } as CSSProperties}
          >
            <img src={framework.src} alt="" />
          </div>
        ))}
      </div>
      <div className="feature-frameworks__providers">
        <span>Static</span>
        <span>Node.js</span>
        <span>Python</span>
        <span>PHP</span>
      </div>
    </div>
  );
}

function OptimizationVisual() {
  const files = [
    { name: "Dev dependencies", size: "620 MB" },
    { name: "Source maps", size: "340 MB" },
    { name: "Tests & fixtures", size: "216 MB" },
  ];

  return (
    <div className="feature-optimize" aria-hidden="true">
      <div className="feature-optimize__grid" />

      <div className="feature-optimize__package feature-optimize__package--source">
        <div className="feature-optimize__header">
          <Package />
          <span>App bundle</span>
          <small>1.2 GB</small>
        </div>
        <div className="feature-optimize__files">
          {files.map(({ name, size }, index) => (
            <div
              key={name}
              className="feature-optimize__file"
              style={{ "--file-index": index } as CSSProperties}
            >
              <span>{name}</span>
              <small>{size}</small>
            </div>
          ))}
          <div className="feature-optimize__file feature-optimize__file--required">
            <Check />
            <span>Runtime files</span>
            <small>24 MB</small>
          </div>
        </div>
      </div>

      <div className="feature-optimize__process">
        <WandSparkles />
        <ArrowRight />
      </div>

      <div className="feature-optimize__package feature-optimize__package--output">
        <div className="feature-optimize__header">
          <Container />
          <span>Runtime only</span>
          <small>24 MB</small>
        </div>
        <div className="feature-optimize__meter">
          <span />
        </div>
        <div className="feature-optimize__result">
          <strong>50×</strong>
          <span>smaller container</span>
        </div>
        <div className="feature-optimize__ready">
          <Check />
          Ready to deploy
        </div>
      </div>
    </div>
  );
}

export function FeatureBento() {
  return (
    <section className="feature-bento" aria-labelledby="feature-highlights-heading">
      <div className="feature-bento__intro">
        <h2
          id="feature-highlights-heading"
          className="text-[38px] font-bold tracking-[-0.03em] text-white sm:text-[46px]"
        >
          Feature Highlights
        </h2>
        <p className="mt-3 text-[15px] text-[#70798D]">
          Every framework, every runtime, everywhere, built and deployed wherever you need.
        </p>
      </div>

      <article className="feature-bento__card feature-bento__card--wide">
        <ZeroConfigVisual />
        <FeatureCopy title="Zero-config builds">
          Detect the project, choose the provider, and generate the complete build plan
          automatically.
        </FeatureCopy>
      </article>

      <article className="feature-bento__card feature-bento__card--narrow">
        <WorkflowVisual />
        <FeatureCopy title="One command workflow">
          Build, package, and prepare every project through one predictable command.
        </FeatureCopy>
      </article>

      <article className="feature-bento__card feature-bento__card--narrow">
        <FrameworkVisual />
        <FeatureCopy title="Framework-aware">
          First-class detection across static sites, Node.js, Python, PHP, and more.
        </FeatureCopy>
      </article>

      <article className="feature-bento__card feature-bento__card--wide">
        <OptimizationVisual />
        <FeatureCopy title="Optimal deployments">
          Anybuild only includes the files and dependencies that are strictly required to run your
          app. This results in up to 50× smaller deployed containers.
        </FeatureCopy>
      </article>
    </section>
  );
}
