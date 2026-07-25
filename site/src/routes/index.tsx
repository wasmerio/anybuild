import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import { ArrowRight, ChevronDown, Copy, Download, ExternalLink, Github } from "lucide-react";
import { BuildFlow } from "@/components/build-flow";
import { FeatureBento } from "@/components/feature-bento";

export const Route = createFileRoute("/")({
  head: () => ({
    meta: [
      { title: "Anybuild — Build websites anywhere. Automatically." },
      {
        name: "description",
        content:
          "Anybuild detects your app, builds it, and deploys it to multiple environments with a simple CLI and zero-config defaults.",
      },
      { property: "og:title", content: "Anybuild — Build websites anywhere. Automatically." },
      {
        property: "og:description",
        content:
          "Zero-config build and deploy CLI. Works with static sites, Astro, Next.js, Vite, and more.",
      },
      { property: "og:type", content: "website" },
      { name: "twitter:card", content: "summary_large_image" },
    ],
  }),
  component: Index,
});

function BrandIcon({ className = "h-7 w-7" }: { className?: string }) {
  return (
    <svg viewBox="270 195 320 285" className={className} aria-hidden="true">
      <defs>
        <linearGradient id="ab-icon" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="#4B7FFF" />
          <stop offset="52%" stopColor="#6348F3" />
          <stop offset="100%" stopColor="#7758FF" />
        </linearGradient>
      </defs>
      <g fill="url(#ab-icon)">
        <path d="M 409 209 L 397 221 L 281 427 L 275 442 L 278 456 L 286 465 L 297 469 L 321 469 L 333 464 L 343 453 L 422 311 L 432 307 L 517 456 L 525 464 L 538 469 L 561 469 L 573 464 L 582 451 L 582 437 L 461 220 L 439 204 L 427 203 Z" />
        <path d="M 401 406 L 396 412 L 394 419 L 394 452 L 396 459 L 401 465 L 406 468 L 410 469 L 448 469 L 454 467 L 461 461 L 464 454 L 464 417 L 462 412 L 457 406 L 449 402 L 410 402 L 404 404 Z" />
      </g>
    </svg>
  );
}

const usageOptions = {
  local: {
    label: "Without sandbox",
    command: "anybuild . --start",
  },
  docker: {
    label: "With Docker",
    command: "anybuild . --docker --start",
  },
  wasmer: {
    label: "With Wasmer",
    command: "anybuild . --wasmer --start",
  },
};

type UsageOption = keyof typeof usageOptions;
const usageOptionOrder: UsageOption[] = ["local", "docker", "wasmer"];

const deploymentOptions = {
  cloudflare: {
    label: "Cloudflare",
    command: "anybuild . --cloudflare-deploy",
  },
  wasmer: {
    label: "Wasmer",
    command: "anybuild . --wasmer-deploy",
  },
  fly: {
    label: "Fly.io",
    command: "anybuild . --fly-deploy",
  },
};

type DeploymentOption = keyof typeof deploymentOptions;
const deploymentOptionOrder: DeploymentOption[] = ["cloudflare", "wasmer", "fly"];

function Index() {
  const [installMethod, setInstallMethod] = useState<"shell" | "cargo">("shell");
  const [usageOption, setUsageOption] = useState<UsageOption>("local");
  const [deploymentOption, setDeploymentOption] = useState<DeploymentOption>("cloudflare");
  const installCommand =
    installMethod === "shell"
      ? "curl -fsSL https://anybuild.sh/install | sh"
      : "cargo install anybuild-cli";
  const usageCommand = usageOptions[usageOption].command;
  const deploymentCommand = deploymentOptions[deploymentOption].command;

  return (
    <div className="relative min-h-screen bg-[#070B17] text-[#F7F9FC]">
      {/* Header */}
      <header className="relative z-10">
        <div className="mx-auto flex h-[72px] max-w-[1240px] items-center justify-between px-6">
          <div className="flex items-center gap-3">
            <BrandIcon className="h-8 w-8" />
            <span className="text-[17px] font-bold tracking-tight text-[#F7F9FC]">Anybuild</span>
            <span className="hidden sm:flex items-center gap-1.5 opacity-20 mt-1 ml-2">
              <span className="text-xs text-[#F7F9FC] font-serif mt-1">by</span>
              <a href="https://wasmer.io" target="_blank" rel="noopener noreferrer">
                <svg viewBox="0 0 623 124" fill="none" height="19" className="inline-block">
                  <path
                    fill="currentColor"
                    d="M.25 45.28c0-7.97 0-11.95 2.14-13.2 2.15-1.23 5.6.76 12.5 4.75l25.92 14.96c6.9 3.99 10.36 5.98 12.5 9.7 2.15 3.71 2.15 7.7 2.15 15.67v29.93c0 7.97 0 11.96-2.14 13.2-2.15 1.23-5.6-.76-12.5-4.75L14.9 100.58C8 96.59 4.54 94.6 2.4 90.88.24 87.19.24 83.19.24 75.22V45.28Z"
                  />
                  <path
                    fill="currentColor"
                    fillRule="evenodd"
                    d="M29.35 16.22c-2.15 1.24-2.15 5.22-2.15 13.2v2.15c.01 0 .03 0 .04.02l25.92 14.96c6.9 3.99 10.35 5.98 12.5 9.7 2.14 3.71 2.14 7.7 2.14 15.67V99.7c6.88 3.97 10.33 5.95 12.47 4.72 2.14-1.24 2.14-5.23 2.14-13.2V61.3c0-7.97 0-11.96-2.14-15.67-2.15-3.72-5.6-5.7-12.5-9.7L41.85 20.97c-6.9-3.99-10.36-5.98-12.5-4.74Z"
                    clipRule="evenodd"
                  />
                  <path
                    fill="currentColor"
                    fillRule="evenodd"
                    d="M56.83.37c-2.15 1.23-2.15 5.22-2.15 13.2v2.14l.04.02L80.64 30.7c6.9 3.98 10.36 5.97 12.5 9.69 2.15 3.71 2.15 7.7 2.15 15.67v27.78c6.88 3.98 10.32 5.96 12.46 4.72 2.14-1.24 2.14-5.22 2.14-13.2V45.45c0-7.97 0-11.96-2.14-15.68-2.14-3.71-5.6-5.7-12.5-9.69L69.33 5.11C62.43 1.12 58.97-.87 56.83.37Z"
                    clipRule="evenodd"
                  />
                  <path
                    fill="currentColor"
                    d="M146.9 28.8a1 1 0 0 0-.94 1.34l23.3 64.4a1 1 0 0 0 .94.66h11.46a1 1 0 0 0 .94-.66l14.46-39.19 14.34 39.19a1 1 0 0 0 .94.66h11.59a1 1 0 0 0 .94-.66l23.44-64.4a1 1 0 0 0-.94-1.34h-15.8a1 1 0 0 0-.95.7l-12.54 38.96-14.15-39a1 1 0 0 0-.94-.66h-11.98a1 1 0 0 0-.94.67l-13.9 38.98-12.65-38.96a1 1 0 0 0-.96-.69H146.9Z"
                  />
                  <path
                    fill="currentColor"
                    fillRule="evenodd"
                    d="M317.1 95.2a1 1 0 0 0 1-1V29.8a1 1 0 0 0-1-1h-14.62a1 1 0 0 0-1 1v5.66a21.94 21.94 0 0 0-6.71-4.97c-4-2-8.29-3-12.83-3-5.89 0-11.3 1.54-16.2 4.62a31.5 31.5 0 0 0-11.55 12.5 36.19 36.19 0 0 0-4.2 17.32 37 37 0 0 0 4.2 17.46A32.43 32.43 0 0 0 265.74 92l.01.01c4.9 3 10.3 4.5 16.2 4.5a28 28 0 0 0 12.68-3 22.01 22.01 0 0 0 6.85-5v5.68a1 1 0 0 0 1 1h14.62Zm-15.5-21.94c-.02 0-.03.02-.04.03 0 .02-.02.04-.04.06l.07-.1Zm-16.63 7.3c-3.52 0-6.68-.8-9.5-2.37a19.06 19.06 0 0 1-6.62-6.75 19.16 19.16 0 0 1-2.37-9.5c0-3.53.8-6.7 2.37-9.52a18 18 0 0 1 6.6-6.6 19.16 19.16 0 0 1 9.52-2.37c3.51 0 6.81.87 9.91 2.63a17.09 17.09 0 0 1 6.6 6.39v19.15a18.34 18.34 0 0 1-6.73 6.3h-.01c-3 1.76-6.26 2.63-9.77 2.63Z"
                    clipRule="evenodd"
                  />
                  <path
                    fill="currentColor"
                    d="M340.83 93.96h.02a50.22 50.22 0 0 0 15.71 2.55c5.03 0 9.52-.85 13.44-2.58h.01c4-1.83 7.13-4.3 9.36-7.45a18.26 18.26 0 0 0 3.48-10.85c0-4.56-1.2-8.33-3.67-11.18a21.64 21.64 0 0 0-8.4-6.2 84.7 84.7 0 0 0-11.86-3.87c-2.7-.7-5-1.34-6.9-1.95a16.74 16.74 0 0 1-4.16-2.2 3.78 3.78 0 0 1-1.23-2.91c0-1.58.65-2.75 2.03-3.63 1.54-.92 3.79-1.43 6.85-1.43 2.85 0 5.98.59 9.41 1.79h.03a23.36 23.36 0 0 1 8.6 4.67 1 1 0 0 0 1.55-.28l6.05-11.72a1 1 0 0 0-.23-1.22c-2.93-2.57-6.66-4.51-11.15-5.86a43.58 43.58 0 0 0-13.6-2.15c-5.03 0-9.56.85-13.57 2.58a21.38 21.38 0 0 0-9.25 7.46 19.18 19.18 0 0 0-3.33 11.1c0 4.56 1.14 8.28 3.53 11.06a21.43 21.43 0 0 0 8.18 5.95 93.92 93.92 0 0 0 11.72 3.33c2.7.6 4.94 1.21 6.75 1.81h.01c1.83.6 3.23 1.36 4.23 2.3l.04.02a3.62 3.62 0 0 1 1.35 2.92c0 1.63-.74 2.96-2.43 4.03-1.72 1.1-4.06 1.69-7.1 1.69a34.6 34.6 0 0 1-11.51-2.18c-4.04-1.46-7.15-3.28-9.4-5.44a1 1 0 0 0-1.57.26l-5.93 11.46a1 1 0 0 0 .24 1.21c3.27 2.83 7.52 5.12 12.7 6.9Z"
                  />
                  <path
                    fill="currentColor"
                    fillRule="evenodd"
                    d="M451.76 95.2a1 1 0 0 0 1-1V52.97a63.5 63.5 0 0 0-.3-1.9 15.82 15.82 0 0 1 5-5.16l.01-.01a14.66 14.66 0 0 1 8.51-2.85c4.16 0 7.29 1.32 9.52 3.87l.01.02c2.35 2.58 3.56 6.1 3.56 10.65v36.6a1 1 0 0 0 1 1h14.88a1 1 0 0 0 1-1V54.83c0-5.4-1.17-10.19-3.55-14.31a23.03 23.03 0 0 0-9.74-9.61 28.46 28.46 0 0 0-14.04-3.41 25.56 25.56 0 0 0-14.38 4.24 34.19 34.19 0 0 0-6.62 5.55 19.64 19.64 0 0 0-7.36-7.04c-3.43-1.85-7.46-2.75-12.07-2.75-4.88 0-9.1 1.04-12.62 3.17a25.23 25.23 0 0 0-6 4.73v-5.6a1 1 0 0 0-1-1h-14.62a1 1 0 0 0-1 1v64.4a1 1 0 0 0 1 1h14.62a1 1 0 0 0 1-1V51.24a15.55 15.55 0 0 1 5.22-5.72 14.78 14.78 0 0 1 8.4-2.47c3.87 0 6.93 1.3 9.26 3.89 2.35 2.58 3.56 6.1 3.56 10.65v36.6a1 1 0 0 0 1 1h14.75Zm3.03-62.63-.55-.84.55.84Zm27.4-.8.45-.88h.02v.01l-.48.88Zm9.34 9.23.88-.48v-.01l-.01-.01-.87.5Zm31.07 51.03h.01a34.48 34.48 0 0 0 17.35 4.48c6.71 0 12.7-1.25 17.95-3.78 5.34-2.54 9.29-6.1 11.77-10.7a1 1 0 0 0-.36-1.33l-11.58-7.1a1 1 0 0 0-1.4.36c-1.29 2.33-3.4 4.22-6.4 5.64h-.01a21.6 21.6 0 0 1-9.57 2.14c-3.8 0-7.13-.84-10.03-2.5a17.24 17.24 0 0 1-6.6-6.96 21.82 21.82 0 0 1-1.83-5.79h48.22a1 1 0 0 0 .99-.87c.17-1.3.27-2.4.27-3.24a119.38 119.38 0 0 0 .26-3.21 31.08 31.08 0 0 0-15.78-27.47 31.92 31.92 0 0 0-16.16-4.21 33.02 33.02 0 0 0-17.23 4.61 33.68 33.68 0 0 0-12.34 12.34v.01a34.23 34.23 0 0 0-4.48 17.22 35.7 35.7 0 0 0 4.47 17.74v.01a34.1 34.1 0 0 0 12.48 12.61Zm1.01-41.12a19.5 19.5 0 0 0-1.17 3.05h32.15a13.9 13.9 0 0 0-2.55-5.69 13.3 13.3 0 0 0-5.52-4.65l-.03-.02a16.43 16.43 0 0 0-7.45-1.73c-3.62 0-6.73.8-9.37 2.36a15.37 15.37 0 0 0-6.06 6.68Z"
                    clipRule="evenodd"
                  />
                  <path
                    fill="currentColor"
                    d="M582.86 28.8a1 1 0 0 0-1 1v64.4a1 1 0 0 0 1 1h14.62a1 1 0 0 0 1-1V53.35a19.34 19.34 0 0 1 6.4-7.31l.02-.01c2.88-2.04 5.5-2.98 7.85-2.98 2.7 0 5.13.42 7.28 1.25a1 1 0 0 0 1.36-.87l.92-13.82a1 1 0 0 0-.68-1.02c-2.23-.74-4.8-1.1-7.7-1.1-3.28 0-6.62 1.03-9.98 3.03l-.01.01a20.58 20.58 0 0 0-5.46 4.77v-5.5a1 1 0 0 0-1-1h-14.62Z"
                  />
                </svg>
              </a>
            </span>
          </div>
          <nav className="flex items-center gap-7 text-[15px] text-[#AEB7C8]">
            <a href="/docs" className="transition-colors hover:text-white">
              Docs
            </a>
            <a
              href="https://github.com/wasmerio/anybuild"
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1.5 transition-colors hover:text-white"
            >
              GitHub <ExternalLink className="h-3.5 w-3.5" />
            </a>
            <a
              href="#install"
              className="inline-flex items-center gap-1.5 rounded-[10px] border border-[#22304D] bg-[#0A1020]/60 px-3.5 py-1.5 transition-colors hover:border-[#3a4a6e] hover:text-white"
            >
              <Download className="h-3.5 w-3.5" /> Download
            </a>
          </nav>
        </div>
      </header>

      {/* Hero */}
      <main className="relative z-10 mx-auto max-w-[1240px] px-6 pt-24 pb-20">
        <section>
          <div className="mx-auto max-w-[900px] text-center">
            <h1 className="text-[52px] font-bold leading-[1.05] tracking-[-0.03em] sm:text-[62px]">
              Build anything.
              <br />
              Deploy anywhere. <span className="text-gradient">Automagically.</span>
            </h1>
            <p className="mx-auto mt-7 max-w-[680px] text-[18px] leading-relaxed text-[#AEB7C8]">
              Anybuild detects your app, builds it, and deploys it to multiple environments with a
              simple CLI and zero-config defaults.
            </p>

            <div className="mt-9 flex flex-wrap items-center justify-center gap-3">
              <a
                href="#install"
                className="bg-brand inline-flex items-center gap-2 rounded-[12px] px-5 py-3 text-[15px] font-semibold text-white transition-transform hover:-translate-y-[1px]"
              >
                Get Started <ArrowRight className="h-4 w-4" />
              </a>
              <a
                href="https://github.com/wasmerio/anybuild"
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-2 rounded-[12px] border border-[#22304D] bg-[#0A1020]/60 px-5 py-3 text-[15px] font-medium text-white transition-colors hover:border-[#3a4a6e]"
              >
                <Github className="h-4 w-4" /> View on GitHub
              </a>
            </div>
          </div>
        </section>

        <BuildFlow logo={<BrandIcon className="h-16 w-16 sm:h-20 sm:w-20" />} />

        {/* Install */}
        <section id="install" className="mt-24 scroll-mt-12 text-center">
          <h2 className="text-[38px] font-bold tracking-[-0.03em] text-white sm:text-[46px]">
            Install Anybuild
          </h2>
          <p className="mt-3 text-[15px] text-[#70798D]">
            Choose your preferred installation method
          </p>

          <div className="mx-auto mt-9 max-w-[900px] overflow-hidden rounded-[18px] border border-[#2A3550] bg-[#050912] shadow-[0_24px_70px_-42px_rgba(0,0,0,0.95)]">
            <div className="grid grid-cols-2 border-b border-[#2A3550] bg-[#0A1020]/70 font-mono text-[14px]">
              <button
                type="button"
                aria-pressed={installMethod === "shell"}
                onClick={() => setInstallMethod("shell")}
                className={`px-6 py-3.5 transition-colors ${
                  installMethod === "shell"
                    ? "bg-[#11192C] text-white"
                    : "text-[#70798D] hover:text-[#AEB7C8]"
                }`}
              >
                macOS / Linux
              </button>
              <button
                type="button"
                aria-pressed={installMethod === "cargo"}
                onClick={() => setInstallMethod("cargo")}
                className={`border-l border-[#2A3550] px-6 py-3.5 transition-colors ${
                  installMethod === "cargo"
                    ? "bg-[#11192C] text-white"
                    : "text-[#70798D] hover:text-[#AEB7C8]"
                }`}
              >
                Cargo
              </button>
            </div>
            <div className="flex min-h-[132px] items-center gap-4 px-6 py-8 text-left sm:px-10">
              <span className="font-mono text-[20px] font-medium text-[#7758FF]">$</span>
              <code className="min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-[15px] text-[#F7F9FC] sm:text-[18px]">
                {installCommand}
              </code>
              <button
                type="button"
                aria-label="Copy install command"
                onClick={() => navigator.clipboard.writeText(installCommand)}
                className="shrink-0 rounded-[9px] border border-[#34415E] p-3 text-[#AEB7C8] transition-colors hover:border-[#526180] hover:bg-white/5 hover:text-white"
              >
                <Copy className="h-5 w-5" />
              </button>
            </div>
          </div>
        </section>

        {/* Usage */}
        <section id="use" className="mt-24 scroll-mt-12 text-center">
          <h2 className="text-[38px] font-bold tracking-[-0.03em] text-white sm:text-[46px]">
            Use anybuild
          </h2>

          <div className="mt-9 grid items-center gap-5 lg:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)]">
            <article className="min-w-0 overflow-hidden rounded-[18px] border border-[#2A3550] bg-[#050912] shadow-[0_24px_70px_-42px_rgba(0,0,0,0.95)]">
              <div className="flex min-h-[62px] items-center justify-between gap-4 border-b border-[#2A3550] bg-[#0A1020]/70 px-5 sm:px-6">
                <span className="font-mono text-[14px] text-[#AEB7C8]">Run locally</span>
                <div className="relative">
                  <select
                    aria-label="Local runtime"
                    value={usageOption}
                    onChange={(event) => setUsageOption(event.target.value as UsageOption)}
                    className="min-w-[170px] appearance-none rounded-[9px] border border-[#34415E] bg-[#050912] py-2 pr-9 pl-3 text-[13px] text-white outline-none transition-colors hover:border-[#526180] focus:border-[#7758FF]"
                  >
                    {usageOptionOrder.map((option) => (
                      <option key={option} value={option}>
                        {usageOptions[option].label}
                      </option>
                    ))}
                  </select>
                  <ChevronDown className="pointer-events-none absolute top-1/2 right-3 h-4 w-4 -translate-y-1/2 text-[#70798D]" />
                </div>
              </div>
              <div className="flex min-h-[132px] items-center gap-4 px-6 py-8 text-left">
                <span className="font-mono text-[20px] font-medium text-[#7758FF]">$</span>
                <code className="min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-[15px] text-[#F7F9FC]">
                  {usageCommand}
                </code>
                <button
                  type="button"
                  aria-label="Copy run command"
                  onClick={() => navigator.clipboard.writeText(usageCommand)}
                  className="shrink-0 rounded-[9px] border border-[#34415E] p-3 text-[#AEB7C8] transition-colors hover:border-[#526180] hover:bg-white/5 hover:text-white"
                >
                  <Copy className="h-5 w-5" />
                </button>
              </div>
            </article>

            <p className="mx-auto max-w-[150px] text-[14px] leading-relaxed text-[#70798D]">
              or deploy it to your favorite provider
            </p>

            <article className="min-w-0 overflow-hidden rounded-[18px] border border-[#2A3550] bg-[#050912] shadow-[0_24px_70px_-42px_rgba(0,0,0,0.95)]">
              <div className="grid min-h-[62px] grid-cols-3 border-b border-[#2A3550] bg-[#0A1020]/70">
                {deploymentOptionOrder.map((option, index) => (
                  <button
                    key={option}
                    type="button"
                    aria-pressed={deploymentOption === option}
                    onClick={() => setDeploymentOption(option)}
                    className={`px-2 text-[13px] transition-colors ${
                      index > 0 ? "border-l border-[#2A3550]" : ""
                    } ${
                      deploymentOption === option
                        ? "bg-[#11192C] text-white"
                        : "text-[#70798D] hover:text-[#AEB7C8]"
                    }`}
                  >
                    {deploymentOptions[option].label}
                  </button>
                ))}
              </div>
              <div className="flex min-h-[132px] items-center gap-4 px-6 py-8 text-left">
                <span className="font-mono text-[20px] font-medium text-[#7758FF]">$</span>
                <code className="min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-[15px] text-[#F7F9FC]">
                  {deploymentCommand}
                </code>
                <button
                  type="button"
                  aria-label="Copy deployment command"
                  onClick={() => navigator.clipboard.writeText(deploymentCommand)}
                  className="shrink-0 rounded-[9px] border border-[#34415E] p-3 text-[#AEB7C8] transition-colors hover:border-[#526180] hover:bg-white/5 hover:text-white"
                >
                  <Copy className="h-5 w-5" />
                </button>
              </div>
            </article>
          </div>
        </section>

        <FeatureBento logo={<BrandIcon className="h-7 w-7" />} />
      </main>

      <footer className="relative z-10 border-t border-[#22304D]/70">
        <div className="mx-auto flex max-w-[1240px] items-center justify-between px-6 py-6 text-[13px] text-[#70798D]">
          <div className="flex items-center gap-2">
            <BrandIcon className="h-5 w-5" />
            <span>Anybuild</span>
            <span className="text-[#70798D]/70">· by wasmer</span>
          </div>
          <span>© {new Date().getFullYear()} Anybuild</span>
        </div>
      </footer>
    </div>
  );
}
