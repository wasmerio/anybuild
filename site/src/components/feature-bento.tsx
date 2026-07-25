import type { CSSProperties, ReactNode } from "react";
import {
  Check,
  Cloud,
  Container,
  Monitor,
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
    src: "https://cdn.jsdelivr.net/gh/devicons/devicon@latest/icons/django/django-plain.svg",
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

function DeploymentVisual({ logo }: { logo: ReactNode }) {
  const targets = [
    { name: "Local", icon: Monitor },
    { name: "Worker", icon: Cloud },
    { name: "Wasmer", icon: Package },
    { name: "Container", icon: Container },
  ];

  return (
    <div className="feature-deploy" aria-hidden="true">
      <div className="feature-deploy__grid" />
      <div className="feature-deploy__source">
        {logo}
        <span>Anybuild</span>
      </div>
      <div className="feature-deploy__trunk">
        <i />
      </div>
      <div className="feature-deploy__targets">
        {targets.map(({ name, icon: Icon }, index) => (
          <div
            key={name}
            className="feature-deploy__target"
            style={{ "--target-index": index } as CSSProperties}
          >
            <Icon />
            <span>{name}</span>
            <Check />
          </div>
        ))}
      </div>
    </div>
  );
}

export function FeatureBento({ logo }: { logo: ReactNode }) {
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
        <DeploymentVisual logo={logo} />
        <FeatureCopy title="Deploy anywhere">
          Turn one project into a local build, Worker, Wasmer package, or container.
        </FeatureCopy>
      </article>
    </section>
  );
}
