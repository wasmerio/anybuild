import { ChevronDown } from "lucide-react";

const questions = [
  {
    question: "Does Anybuild require Docker?",
    answer: (
      <p>
        No. Local building and local preview are the defaults. Docker is optional and only selected
        with <code>--docker</code> or <code>--docker-client</code>.
      </p>
    ),
  },
  {
    question: "Does --docker run the application in a container?",
    answer: (
      <p>
        No. Docker is a build backend: it executes build steps in an isolated container and exports
        artifacts into <code>.anybuild</code>. Runtime selection is separate; use the local runner
        or add <code>--wasmer</code>.
      </p>
    ),
  },
  {
    question: "What if detection chooses the wrong provider?",
    answer: (
      <>
        <p>Force a registered provider, inspect the plan, and regenerate the project definition.</p>
        <pre>
          <code>{`anybuild plan . --provider node
anybuild . --provider node --regenerate --start`}</code>
        </pre>
      </>
    ),
  },
  {
    question: "Which platforms can Anybuild deploy to directly?",
    answer: (
      <p>
        The current Rust implementation publishes directly to Wasmer or writes Wasmer deployment
        configuration. Other platforms can integrate the CLI or SDK and consume the generated plan
        and artifacts, but Anybuild does not currently expose generic Cloudflare, Fly.io, or
        OCI-image deployment commands.
      </p>
    ),
  },
  {
    question: "Can Anybuild be used without the CLI?",
    answer: (
      <p>
        Yes. The <code>anybuild</code> Rust crate exposes the same synchronous generate, plan,
        build, run, deploy, and auto operations with structured options, outcomes, errors, and
        events.
      </p>
    ),
  },
];

export function Faq() {
  return (
    <section className="faq" aria-labelledby="faq-heading">
      <div className="faq__intro">
        <h2 id="faq-heading">Frequently Asked Questions</h2>
        <p>Short answers about Anybuild’s detection, environments, compatibility, and scope.</p>
      </div>

      <div className="faq__list">
        {questions.map(({ question, answer }) => (
          <details className="faq__item" key={question}>
            <summary>
              <span>{question}</span>
              <ChevronDown aria-hidden="true" />
            </summary>
            <div className="faq__answer">{answer}</div>
          </details>
        ))}
      </div>
    </section>
  );
}
