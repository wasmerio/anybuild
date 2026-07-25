import { ArrowLeft, ArrowRight, Github, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { docHref, docsNav, docsPages, type DocPage, type DocsNavGroup } from "@/lib/docs-content";

function DocsBrandIcon({ className = "h-7 w-7" }: { className?: string }) {
  return (
    <svg viewBox="270 195 320 285" className={className} aria-hidden="true">
      <defs>
        <linearGradient id="ab-docs-icon" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="#4B7FFF" />
          <stop offset="52%" stopColor="#6348F3" />
          <stop offset="100%" stopColor="#7758FF" />
        </linearGradient>
      </defs>
      <g fill="url(#ab-docs-icon)">
        <path d="M 409 209 L 397 221 L 281 427 L 275 442 L 278 456 L 286 465 L 297 469 L 321 469 L 333 464 L 343 453 L 422 311 L 432 307 L 517 456 L 525 464 L 538 469 L 561 469 L 573 464 L 582 451 L 582 437 L 461 220 L 439 204 L 427 203 Z" />
        <path d="M 401 406 L 396 412 L 394 419 L 394 452 L 396 459 L 401 465 L 406 468 L 410 469 L 448 469 L 454 467 L 461 461 L 464 454 L 464 417 L 462 412 L 457 406 L 449 402 L 410 402 L 404 404 Z" />
      </g>
    </svg>
  );
}

function filterNavigation(query: string): DocsNavGroup[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return docsNav;
  return docsNav
    .map((group) => ({
      ...group,
      items: group.items.filter((item) => item.title.toLowerCase().includes(normalized)),
    }))
    .filter((group) => group.items.length > 0);
}

function DocsNavigation({ groups, activeSlug }: { groups: DocsNavGroup[]; activeSlug: string }) {
  return (
    <nav aria-label="Documentation">
      {groups.map((group) => (
        <div key={group.title} className="mb-7">
          <h2 className="mb-2 px-3 text-[11px] font-semibold uppercase tracking-[0.14em] text-[#59647A]">
            {group.title}
          </h2>
          <ul className="space-y-0.5">
            {group.items.map((item) => {
              const active = activeSlug === item.slug;
              return (
                <li key={item.slug}>
                  <a
                    href={docHref(item.slug)}
                    aria-current={active ? "page" : undefined}
                    className={`block rounded-[8px] px-3 py-2 text-[13px] transition-colors ${
                      active
                        ? "bg-[#151E36] font-medium text-white"
                        : "text-[#8C96AA] hover:bg-[#0A1020] hover:text-[#DCE2EE]"
                    }`}
                  >
                    {item.title}
                  </a>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </nav>
  );
}

export function DocsShell({ page }: { page: DocPage }) {
  const [query, setQuery] = useState("");
  const filteredNavigation = useMemo(() => filterNavigation(query), [query]);
  const pageIndex = docsPages.findIndex((candidate) => candidate.slug === page.slug);
  const previous = pageIndex > 0 ? docsPages[pageIndex - 1] : undefined;
  const next = pageIndex >= 0 ? docsPages[pageIndex + 1] : undefined;

  return (
    <div className="min-h-screen bg-[#070B17] text-[#F7F9FC]">
      <header className="sticky top-0 z-30 border-b border-[#22304D]/75 bg-[#070B17]/90 backdrop-blur-xl">
        <div className="mx-auto flex h-[66px] max-w-[1440px] items-center justify-between px-5 sm:px-7">
          <a href="/" className="flex items-center gap-2.5">
            <DocsBrandIcon className="h-7 w-7" />
            <span className="text-[15px] font-bold tracking-tight">Anybuild</span>
            <span className="h-4 w-px bg-[#34415E]" />
            <span className="text-[13px] text-[#8C96AA]">Docs</span>
          </a>
          <div className="flex items-center gap-4 text-[13px] text-[#8C96AA]">
            <a href="/" className="transition-colors hover:text-white">
              Homepage
            </a>
            <a
              href="https://github.com/wasmerio/anybuild"
              target="_blank"
              rel="noreferrer"
              aria-label="Anybuild on GitHub"
              className="transition-colors hover:text-white"
            >
              <Github className="h-4 w-4" />
            </a>
          </div>
        </div>
      </header>

      <div className="mx-auto max-w-[1440px] px-5 sm:px-7">
        <details className="mt-5 rounded-[12px] border border-[#22304D] bg-[#0A1020]/60 p-4 lg:hidden">
          <summary className="cursor-pointer text-[14px] font-medium text-white">
            Documentation navigation
          </summary>
          <div className="mt-4 border-t border-[#22304D] pt-4">
            <DocsNavigation groups={docsNav} activeSlug={page.slug} />
          </div>
        </details>

        <div className="grid gap-12 lg:grid-cols-[230px_minmax(0,760px)] xl:grid-cols-[230px_minmax(0,760px)_190px] xl:gap-16">
          <aside className="hidden lg:block">
            <div className="sticky top-[90px] max-h-[calc(100vh-110px)] overflow-y-auto py-8 pr-3">
              <label className="relative mb-7 block">
                <Search className="pointer-events-none absolute top-1/2 left-3 h-3.5 w-3.5 -translate-y-1/2 text-[#59647A]" />
                <input
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="Search pages"
                  className="w-full rounded-[9px] border border-[#22304D] bg-[#0A1020] py-2 pr-3 pl-9 text-[12px] text-white outline-none placeholder:text-[#59647A] focus:border-[#6348F3]"
                />
              </label>
              {filteredNavigation.length ? (
                <DocsNavigation groups={filteredNavigation} activeSlug={page.slug} />
              ) : (
                <p className="px-3 text-[12px] text-[#70798D]">No matching pages.</p>
              )}
            </div>
          </aside>

          <main className="min-w-0 py-10 sm:py-14">
            <div className="mb-3 font-mono text-[11px] uppercase tracking-[0.16em] text-[#7758FF]">
              Anybuild documentation
            </div>
            <h1 className="text-[38px] font-bold leading-[1.1] tracking-[-0.04em] text-white sm:text-[48px]">
              {page.title}
            </h1>
            <p className="mt-5 max-w-[680px] text-[17px] leading-8 text-[#8C96AA]">
              {page.description}
            </p>

            <div className="mt-12">
              {page.sections.map((section, index) => (
                <section
                  key={section.id}
                  id={section.id}
                  className={`scroll-mt-24 ${
                    index === 0 ? "" : "mt-12 border-t border-[#22304D]/70 pt-10"
                  }`}
                >
                  <h2 className="text-[24px] font-semibold tracking-[-0.025em] text-white">
                    {section.title}
                  </h2>
                  {section.content}
                </section>
              ))}
            </div>

            <nav
              aria-label="Previous and next documentation pages"
              className="mt-16 grid gap-4 border-t border-[#22304D] pt-8 sm:grid-cols-2"
            >
              {previous ? (
                <a
                  href={docHref(previous.slug)}
                  className="group rounded-[12px] border border-[#22304D] px-4 py-3 transition-colors hover:border-[#3A4A6E] hover:bg-[#0A1020]"
                >
                  <span className="flex items-center gap-1.5 text-[11px] uppercase tracking-[0.12em] text-[#59647A]">
                    <ArrowLeft className="h-3.5 w-3.5" /> Previous
                  </span>
                  <strong className="mt-1 block text-[14px] font-medium text-[#AEB7C8] group-hover:text-white">
                    {previous.title}
                  </strong>
                </a>
              ) : (
                <span />
              )}
              {next && (
                <a
                  href={docHref(next.slug)}
                  className="group rounded-[12px] border border-[#22304D] px-4 py-3 text-right transition-colors hover:border-[#3A4A6E] hover:bg-[#0A1020]"
                >
                  <span className="flex items-center justify-end gap-1.5 text-[11px] uppercase tracking-[0.12em] text-[#59647A]">
                    Next <ArrowRight className="h-3.5 w-3.5" />
                  </span>
                  <strong className="mt-1 block text-[14px] font-medium text-[#AEB7C8] group-hover:text-white">
                    {next.title}
                  </strong>
                </a>
              )}
            </nav>
          </main>

          <aside className="hidden xl:block">
            <div className="sticky top-[98px] py-10">
              <h2 className="text-[11px] font-semibold uppercase tracking-[0.14em] text-[#59647A]">
                On this page
              </h2>
              <nav className="mt-3 border-l border-[#22304D]">
                {page.sections.map((section) => (
                  <a
                    key={section.id}
                    href={`#${section.id}`}
                    className="block border-l border-transparent py-1.5 pl-4 text-[12px] leading-5 text-[#70798D] transition-colors hover:border-[#7758FF] hover:text-[#DCE2EE]"
                  >
                    {section.title}
                  </a>
                ))}
              </nav>
            </div>
          </aside>
        </div>
      </div>
    </div>
  );
}
