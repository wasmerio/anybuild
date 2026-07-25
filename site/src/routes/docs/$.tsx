import { createFileRoute, notFound } from "@tanstack/react-router";
import { DocsShell } from "@/components/docs-shell";
import { getDocPage } from "@/lib/docs-content";

export const Route = createFileRoute("/docs/$")({
  loader: ({ params }) => {
    if (!getDocPage(params._splat)) throw notFound();
    return params._splat;
  },
  head: ({ params }) => {
    const page = getDocPage(params._splat);
    return {
      meta: [
        { title: page ? `${page.title} — Anybuild Docs` : "Anybuild Docs" },
        {
          name: "description",
          content: page?.description ?? "Anybuild documentation.",
        },
      ],
    };
  },
  component: DocsSplat,
});

function DocsSplat() {
  const slug = Route.useLoaderData();
  const page = getDocPage(slug);
  if (!page) throw notFound();
  return <DocsShell page={page} />;
}
