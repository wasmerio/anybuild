import { createFileRoute } from "@tanstack/react-router";
import { DocsShell } from "@/components/docs-shell";
import { getDocPage } from "@/lib/docs-content";

const page =
  getDocPage("") ??
  (() => {
    throw new Error("Getting Started documentation page is missing");
  })();

export const Route = createFileRoute("/docs/")({
  head: () => ({
    meta: [
      { title: "Getting Started — Anybuild Docs" },
      {
        name: "description",
        content:
          "Install Anybuild, detect a project, and run the complete build pipeline locally, in Docker, or with Wasmer.",
      },
    ],
  }),
  component: DocsIndex,
});

function DocsIndex() {
  return <DocsShell page={page} />;
}
